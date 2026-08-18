//! The IPC surface. Kept thin on purpose: the popup should be able to describe
//! what happened in one call and then get out of the way.

use tauri::{Manager, State};

use crate::db::{self, intentions, sessions, tasks, Db};

#[tauri::command]
pub fn ping() -> String {
    "Core responded.".to_string()
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RitualResult {
    pub id: i64,
    /// The session this ritual started, when it committed to a length.
    pub session_id: Option<i64>,
    /// Present when the session asked for sites to be blocked. `blocked: false`
    /// carries a reason the confirmation screen shows, because telling someone
    /// their sites are blocked when they are not is worse than not blocking.
    pub block: Option<crate::session::BlockOutcome>,
}

/// The longest commitment a single ritual may make.
///
/// Matches `MAX_DURATION` in the frontend's copy, and is enforced here as well
/// because the frontend is not a trust boundary. Beyond two hours this stops
/// being a focus session and becomes a Pause with extra steps - and Pause
/// already exists, with its own screen and its own way out.
const MAX_DURATION_MIN: i64 = 120;

/// Record what the user did, arm any block they committed to, then close.
///
/// The record is written first so that a failure further along loses the block,
/// never the answer. Arming happens before the window closes so the outcome can
/// still be shown on the confirmation screen.
#[tauri::command]
pub fn finish_ritual(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    mut intention: intentions::NewIntention,
    custom_sites: Option<String>,
) -> Result<RitualResult, String> {
    let now = chrono::Utc::now().timestamp();
    let minutes = intention
        .duration_min
        .filter(|m| *m > 0)
        .map(|m| m.min(MAX_DURATION_MIN));

    let categories: Vec<String> = intention
        .categories
        .as_deref()
        .unwrap_or_default()
        .split(',')
        .filter(|c| !c.is_empty())
        .map(str::to_owned)
        .collect();

    // Sites the user typed. Normalised again here rather than trusted from the
    // window: the frontend is not a trust boundary, and the helper will refuse
    // the whole request over one bad name — which would read as "blocking is
    // broken" rather than "that one is not a web address".
    let (custom, rejected) = split_sites(custom_sites.as_deref());

    let (id, session_id) = {
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;

        // Foreign keys are on, so an id that no longer resolves - the task was
        // deleted while the ritual was open - would fail the insert. Losing the
        // whole record over a link is the wrong trade: the intention is what
        // matters, the link is an extra.
        // The subject the banner and the check-in will name. A task's title
        // when there is a task; otherwise what was typed - a session started
        // from an intention was still started for something, and a check-in
        // that says only "90 minutes." has forgotten what it was asking about.
        let task_title = match intention.task_id {
            Some(task_id) => match tasks::title_of(&conn, task_id) {
                Ok(title) => Some(title),
                Err(_) => {
                    intention.task_id = None;
                    None
                }
            },
            None => None,
        }
        .or_else(|| {
            intention
                .text
                .as_deref()
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
        });

        // One transaction: a session without its intention is unrecoverable,
        // and the two are written together or not at all.
        let tx = conn
            .unchecked_transaction()
            .map_err(|err| format!("could not begin: {err}"))?;

        // A row left open by a run that was killed would take the one running
        // slot with it, and the unique index would then refuse this insert.
        if let Some(stale) = sessions::live(&tx)? {
            sessions::mark(&tx, stale.id, sessions::State::Unanswered, Some(now))?;
        }

        let id = intentions::insert(&tx, &intention)?;

        // The decoupling: a length is all it takes. Blocking is enforcement on
        // top, and choosing none used to mean the whole ritual produced nothing
        // but a database row.
        let session_id = match minutes {
            Some(minutes) => Some(sessions::start(
                &tx,
                &sessions::NewSession {
                    intention_id: id,
                    task_id: intention.task_id,
                    task_title,
                    duration_min: minutes,
                    categories: intention.categories.clone(),
                    predicted_yes: intention.predicted_yes,
                },
                now,
            )?),
            // The honourable exit commits to nothing, so there is nothing to run.
            None => None,
        };

        tx.commit().map_err(|err| format!("could not save: {err}"))?;
        (id, session_id)
    };

    // Arming is outside the transaction: it waits on an elevated process for up
    // to twelve seconds, and holding a database lock across that would freeze
    // every other reader.
    let nothing_to_block = categories.is_empty() && custom.is_empty();
    let block = match (nothing_to_block, minutes) {
        // One unusable site name fails the block rather than being dropped in
        // silence: a blocklist that quietly omits something is worse than one
        // that says it could not.
        (_, Some(_)) if !rejected.is_empty() => Some(crate::session::BlockOutcome::failed(
            format!("This is not a web address: {}", rejected.join(", ")),
        )),
        (false, Some(minutes)) => {
            let outcome = crate::session::arm(&categories, &custom, minutes);
            if outcome.blocked {
                if let (Some(session_id), Some(lock)) = (session_id, crate::session::active_lock())
                {
                    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
                    sessions::confirm_block(&conn, session_id, lock.locked_until)?;
                }
            }
            Some(outcome)
        }
        _ => None,
    };

    // Tell the debounce, whether or not a block was armed - but tell it which,
    // because only an enforced session silences boot, wake and unlock.
    if let Some(minutes) = minutes {
        let enforced = block.as_ref().is_some_and(|outcome| outcome.blocked);
        crate::triggers::note_session_until(now + minutes * 60, enforced);
    }

    if let Some(session_id) = session_id {
        crate::session::announce_started(&app, session_id);
    }

    // A failed block keeps the window open so the reason is read rather than
    // flashing past on the way out.
    let keep_open = block.as_ref().is_some_and(|outcome| !outcome.blocked);
    if !keep_open {
        crate::popup::close(&app).map_err(|err| err.to_string())?;
    }

    Ok(RitualResult {
        id,
        session_id,
        block,
    })
}

/// Chips offered on the intention step.
///
/// Sourced from the "Do First" quadrant, which is the entire point of pairing
/// the list with the ritual: at the vulnerable moment you are shown what
/// matters instead of being asked to remember it. Recent intentions fill any
/// remaining slots so the step is never empty on a fresh install.
/// A chip on the intention step: the text, and the task behind it if there is
/// one. Chips drawn from history have no task to point at.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    pub task_id: Option<i64>,
    pub title: String,
}

#[tauri::command]
pub fn intention_suggestions(db: State<'_, Db>) -> Result<Vec<Suggestion>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;

    let mut chips: Vec<Suggestion> = tasks::do_first(&conn, 3)?
        .into_iter()
        .map(|(task_id, title)| Suggestion {
            task_id: Some(task_id),
            title,
        })
        .collect();

    if chips.len() < 3 {
        for text in intentions::recent_texts(&conn, 3)? {
            if chips.len() >= 3 {
                break;
            }
            if !chips.iter().any(|chip| chip.title == text) {
                chips.push(Suggestion {
                    task_id: None,
                    title: text,
                });
            }
        }
    }
    Ok(chips)
}

#[tauri::command]
pub fn list_contexts(db: State<'_, Db>) -> Result<Vec<tasks::Context>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    tasks::contexts(&conn)
}

/// The areas, as stored, after a change - so the toolbar never guesses.
#[tauri::command]
pub fn add_context(db: State<'_, Db>, name: String) -> Result<Vec<tasks::Context>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    tasks::add_context(&conn, &name)?;
    tasks::contexts(&conn)
}

#[tauri::command]
pub fn rename_context(
    db: State<'_, Db>,
    id: i64,
    name: String,
) -> Result<Vec<tasks::Context>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    tasks::rename_context(&conn, id, &name)?;
    tasks::contexts(&conn)
}

#[tauri::command]
pub fn remove_context(db: State<'_, Db>, id: i64) -> Result<Vec<tasks::Context>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    tasks::remove_context(&conn, id)?;
    tasks::contexts(&conn)
}

#[tauri::command]
pub fn set_task_context(
    db: State<'_, Db>,
    id: i64,
    context_id: Option<i64>,
) -> Result<(), String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    tasks::set_context(&conn, id, context_id)
}

#[tauri::command]
pub fn list_tasks(db: State<'_, Db>) -> Result<Vec<tasks::Task>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    tasks::open_tasks(&conn)
}

#[tauri::command]
pub fn add_task(db: State<'_, Db>, task: tasks::NewTask) -> Result<i64, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    tasks::add(&conn, &task)
}

#[tauri::command]
pub fn move_task(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    id: i64,
    urgent: Option<bool>,
    important: Option<bool>,
    sort_order: Option<i64>,
) -> Result<(), String> {
    let before = {
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        let before = schedule_state(&conn, id);
        tasks::set_quadrant(&conn, id, urgent, important, sort_order.unwrap_or(0))?;
        before
    };
    reconcile_schedule(&app, &db, id, before);
    Ok(())
}

/// Whether a task is *actively* in Schedule: the quadrant's flags and still
/// open. A done or deleted task keeps its flags but is no longer scheduled, so
/// completing one is a departure the calendar must hear about.
fn schedule_state(conn: &rusqlite::Connection, id: i64) -> bool {
    tasks::by_id(conn, id).ok().flatten().is_some_and(|t| {
        t.urgent == Some(false) && t.important == Some(true) && t.status == "open"
    })
}

/// After a task changed, tell the calendar if it entered or left Schedule.
/// Off the main thread; never fails the change that triggered it.
fn reconcile_schedule(app: &tauri::AppHandle, db: &State<'_, Db>, id: i64, before: bool) {
    let after = {
        match db.0.lock() {
            Ok(conn) => schedule_state(&conn, id),
            Err(_) => return,
        }
    };
    if before && !after {
        crate::calendar::sync::on_task_left_schedule(app.clone(), id);
    } else if !before && after {
        crate::calendar::sync::on_task_entered_schedule(app.clone(), id);
    }
}

/// The order of one zone, as the user left it. Ids get their index; everything
/// else keeps its number.
#[tauri::command]
pub fn reorder_tasks(db: State<'_, Db>, ids: Vec<i64>) -> Result<(), String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    tasks::reorder(&conn, &ids)
}

#[tauri::command]
pub fn set_task_status(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    id: i64,
    status: String,
) -> Result<(), String> {
    let before = {
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        let before = schedule_state(&conn, id);
        tasks::set_status(&conn, id, &status)?;
        before
    };
    reconcile_schedule(&app, &db, id, before);
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusResult {
    pub opened: bool,
    /// Why nothing happened, when nothing happened. Previously this command
    /// returned Ok(()) whether or not it did anything, so a paused app made the
    /// button look broken.
    pub reason: Option<String>,
}

/// Start a focus session from a task, without waiting for a boot or wake.
///
/// Takes the task so the ritual can open with that intention already filled in
/// — otherwise "Focus on this" is indistinguishable from "open the ritual".
/// `async` is load-bearing, not decoration.
///
/// A synchronous command runs *on the main thread*, which is the thread the
/// event loop needs in order to create a window and attach a WebView2 to it.
/// Building the ritual from inside one leaves a half-made window: the Win32
/// window exists, so it can be enumerated, but it never becomes visible and the
/// code after the build never runs. That was "the Focus button does nothing" —
/// every click created an invisible fullscreen window and stopped there.
///
/// Triggers were never affected: they arrive on the event loop rather than
/// occupying it, which is why boot, wake and unlock always worked.
#[tauri::command(async)]
pub fn focus_on_task(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    task_id: i64,
) -> Result<FocusResult, String> {
    let title = {
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        tasks::title_of(&conn, task_id)?
    };

    let prefill = crate::popup::Prefill {
        intent: Some(title),
        task_id: Some(task_id),
        express: true,
    };

    match crate::triggers::request_focus(&app, prefill) {
        Ok(()) => Ok(FocusResult {
            opened: true,
            reason: None,
        }),
        Err(reason) => Ok(FocusResult {
            opened: false,
            reason: Some(reason),
        }),
    }
}

#[tauri::command]
pub fn diagnostics(db: State<'_, Db>) -> Result<crate::diagnostics::Diagnostics, String> {
    let paused = {
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        crate::pause::paused_until(&conn)
    };
    Ok(crate::diagnostics::diagnostics_with_pause(paused))
}

/// Keep the native titlebar in step with the chosen appearance.
///
/// `None` means follow Windows, which also restores the webview's automatic
/// colour scheme — so a "system" preference keeps tracking live OS changes
/// through `prefers-color-scheme`. The two mechanisms are never both in play,
/// which is why they cannot disagree.
#[tauri::command]
pub fn set_window_theme(app: tauri::AppHandle, theme: Option<String>) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    let _ = window.set_theme(match theme.as_deref() {
        Some("light") => Some(tauri::Theme::Light),
        Some("dark") => Some(tauri::Theme::Dark),
        _ => None,
    });
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveSession {
    pub id: i64,
    pub task_id: Option<i64>,
    /// The task's title as it was when the session began, or the typed
    /// intention when there was no task.
    pub subject: Option<String>,
    pub started_ts: i64,
    pub ends_ts: i64,
    pub duration_min: i64,
    /// Advisory. The UI recomputes from `endsTs` on every tick rather than
    /// trusting this, because a number sent once goes stale across a sleep.
    pub seconds_remaining: i64,
    /// Live, from the lock - not the stored flag, which only records what was
    /// true at arm time. A block may have been lifted since.
    pub blocking: bool,
    pub categories: Vec<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStatus {
    pub session: Option<ActiveSession>,
    /// Seconds the sites stay quiet, whether or not a session is behind it.
    /// Present on its own after an upgrade mid-commitment, or when a block
    /// outlives the session that armed it.
    pub block_seconds: Option<i64>,
}

/// Split a saved site list into the ones we can block and the ones we cannot.
///
/// Normalising here as well as in the UI is not belt-and-braces for its own
/// sake: the stored list outlives the window that wrote it, and a name that was
/// acceptable to an older build must not reach an elevated process unchecked.
pub fn split_sites(raw: Option<&str>) -> (Vec<String>, Vec<String>) {
    let mut good = Vec::new();
    let mut bad = Vec::new();

    for piece in raw.unwrap_or_default().split(',') {
        if piece.trim().is_empty() {
            continue;
        }
        let host = threshold_protocol::normalise_host(piece);
        match threshold_protocol::valid_hostname(&host) {
            Ok(()) if !good.contains(&host) => good.push(host),
            Ok(()) => {}
            Err(_) => bad.push(piece.trim().to_string()),
        }
    }

    good.truncate(threshold_protocol::MAX_CUSTOM_HOSTS);
    (good, bad)
}

fn split_categories(raw: Option<&str>) -> Vec<String> {
    raw.unwrap_or_default()
        .split(',')
        .filter(|c| !c.is_empty())
        .map(str::to_owned)
        .collect()
}

/// What is running, if anything.
#[tauri::command]
pub fn session_status(db: State<'_, Db>) -> Result<SessionStatus, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    let now = chrono::Utc::now().timestamp();
    let lock = crate::session::active_lock();
    let block_seconds = lock
        .as_ref()
        .map(crate::session::seconds_remaining)
        .filter(|left| *left > 0);

    let session = sessions::live(&conn)?
        .filter(|row| row.ends_ts > now)
        .map(|row| ActiveSession {
            id: row.id,
            task_id: row.task_id,
            subject: row
                .task_title
                .clone()
                .or_else(|| intentions::text_of(&conn, row.intention_id)),
            started_ts: row.started_ts,
            ends_ts: row.ends_ts,
            duration_min: row.duration_min,
            seconds_remaining: (row.ends_ts - now).max(0),
            blocking: block_seconds.is_some(),
            categories: split_categories(row.categories.as_deref()),
        });

    Ok(SessionStatus {
        session,
        block_seconds,
    })
}

/// A finished session, as the Overview reads it.
///
/// Trimmed to what a summary needs. `state` travels as the raw string so the
/// frontend can tell an unanswered session from a "no" — the distinction the
/// whole check-in depends on, and the one a boolean would destroy.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub started_ts: i64,
    /// When it actually stopped. `None` for one still running.
    pub ended_ts: Option<i64>,
    /// What was committed to. Not what was spent — those differ, and conflating
    /// them is what let an abandoned ninety-minute session count as ninety
    /// minutes reclaimed.
    pub duration_min: i64,
    pub predicted_yes: Option<bool>,
    pub state: String,
}

#[tauri::command]
pub fn recent_sessions(
    db: State<'_, Db>,
    limit: Option<i64>,
) -> Result<Vec<SessionRecord>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    Ok(sessions::recent(&conn, limit.unwrap_or(200))?
        .into_iter()
        .map(|row| SessionRecord {
            started_ts: row.started_ts,
            ended_ts: row.ended_ts,
            duration_min: row.duration_min,
            predicted_yes: row.predicted_yes,
            state: row.state.as_str().to_string(),
        })
        .collect())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndSessionResult {
    /// The commitment lock is the point of the product, so ending a session
    /// does not lift a block that still owes time. `Some(n)` means the sites
    /// stay quiet for another n seconds, and the UI has to say so rather than
    /// removing the banner and leaving no explanation for why Reddit is broken.
    pub block_held_secs: Option<i64>,
}

/// Stop a session before its time. The block, if any, keeps its own schedule.
#[tauri::command]
pub fn end_session_early(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    session_id: i64,
) -> Result<EndSessionResult, String> {
    let now = chrono::Utc::now().timestamp();
    {
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        sessions::mark(&conn, session_id, sessions::State::AwaitingCheckin, Some(now))?;
    }
    crate::triggers::clear_session();
    crate::session::announce_ended(&app, session_id, "early");

    Ok(EndSessionResult {
        block_held_secs: crate::session::active_lock()
            .map(|lock| crate::session::seconds_remaining(&lock))
            .filter(|left| *left > 0),
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCheckin {
    pub session_id: i64,
    pub task_id: Option<i64>,
    /// Only offered when the task still exists and is still open. Ticking a
    /// deleted task "done" would resurrect it into the matrix.
    pub can_mark_done: bool,
    pub subject: Option<String>,
    pub predicted_yes: Option<bool>,
    /// Minutes actually elapsed, not minutes committed. A session ended after
    /// nine minutes says nine.
    pub minutes: i64,
    /// Seconds since it ran out. Non-zero means the machine was asleep or the
    /// user was away.
    pub late_by: i64,
    pub block_held_secs: Option<i64>,
}

/// The question owed, if one is.
#[tauri::command]
pub fn pending_checkin(db: State<'_, Db>) -> Result<Option<PendingCheckin>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    let now = chrono::Utc::now().timestamp();

    let Some(row) = sessions::awaiting(&conn)? else {
        return Ok(None);
    };

    let ended = row.ended_ts.unwrap_or(row.ends_ts);
    let can_mark_done = match row.task_id {
        Some(task_id) => tasks::is_open(&conn, task_id).unwrap_or(false),
        None => false,
    };

    // Sessions recorded before the subject was carried over have none stored;
    // their intention still knows what they were for.
    let subject = row
        .task_title
        .clone()
        .or_else(|| intentions::text_of(&conn, row.intention_id));

    Ok(Some(PendingCheckin {
        session_id: row.id,
        task_id: row.task_id,
        can_mark_done,
        subject,
        predicted_yes: row.predicted_yes,
        minutes: ((ended - row.started_ts).max(0) + 30) / 60,
        late_by: (now - ended).max(0),
        block_held_secs: crate::session::active_lock()
            .map(|lock| crate::session::seconds_remaining(&lock))
            .filter(|left| *left > 0),
    }))
}

/// Record how it went, and optionally tick the task off in the same breath.
#[tauri::command]
pub fn answer_checkin(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    session_id: i64,
    answer: sessions::Answer,
    mark_task_done: bool,
) -> Result<(), String> {
    let now = chrono::Utc::now().timestamp();
    {
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|err| format!("could not begin: {err}"))?;

        let row = sessions::by_id(&tx, session_id)?
            .ok_or_else(|| format!("no session with id {session_id}"))?;

        // Same transaction, so a task marked done always has a session row
        // saying why. Re-checked here rather than trusted from the UI: the task
        // may have been deleted while the question sat on screen.
        let done = mark_task_done
            && match row.task_id {
                Some(task_id) if tasks::is_open(&tx, task_id).unwrap_or(false) => {
                    tasks::set_status(&tx, task_id, "done")?;
                    true
                }
                _ => false,
            };

        sessions::answer(&tx, session_id, answer, done, now)?;
        tx.commit().map_err(|err| format!("could not save: {err}"))?;
    }
    crate::checkin::close(&app);
    crate::session::announce_ended(&app, session_id, "answered");
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContinueResult {
    pub session_id: i64,
    /// Present when the last slice was enforced or chose categories - the same
    /// selections, re-armed. `blocked: false` carries the reason.
    pub block: Option<crate::session::BlockOutcome>,
}

/// "Partly - keep going now": record the answer, then start another slice on
/// the same subject with the same selections, only the length asked again.
///
/// The prediction is deliberately not carried over: the check-in scores answers
/// against predictions, and a prediction nobody re-made must not be re-scored.
/// `async` because arming waits on the elevated helper for up to twelve
/// seconds, which would freeze every window from the main thread.
#[tauri::command(async)]
pub fn continue_session(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    session_id: i64,
    answer: sessions::Answer,
    minutes: i64,
) -> Result<ContinueResult, String> {
    let now = chrono::Utc::now().timestamp();
    let minutes = minutes.clamp(1, MAX_DURATION_MIN);

    let (new_id, categories, custom, should_block) = {
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|err| format!("could not begin: {err}"))?;

        let row = sessions::by_id(&tx, session_id)?
            .ok_or_else(|| format!("no session with id {session_id}"))?;

        // The answer first, the continuation second: a failure further along
        // must lose the new slice, never what was said about the old one.
        sessions::answer(&tx, session_id, answer, false, now)?;

        // Same guard as finish_ritual: a row left running by a killed process
        // holds the one running slot and would refuse the insert.
        if let Some(stale) = sessions::live(&tx)? {
            sessions::mark(&tx, stale.id, sessions::State::Unanswered, Some(now))?;
        }

        let new_id = sessions::start(
            &tx,
            &sessions::NewSession {
                intention_id: row.intention_id,
                task_id: row.task_id,
                task_title: row.task_title.clone(),
                duration_min: minutes,
                categories: row.categories.clone(),
                predicted_yes: None,
            },
            now,
        )?;
        tx.commit().map_err(|err| format!("could not save: {err}"))?;

        let categories = split_categories(row.categories.as_deref());
        // The sites ride along from the saved list, exactly as a new ritual
        // would read them - the session row never stored them.
        let saved = db::get_setting(&conn, "custom_sites").unwrap_or_default();
        let (custom, _) = split_sites(Some(saved.as_str()));

        // Block again only if the last slice blocked, or at least asked to:
        // continuing must not conjure an enforcement nobody selected.
        let should_block = row.enforced || !split_categories(row.categories.as_deref()).is_empty();
        (new_id, categories, custom, should_block)
    };

    let block = if should_block && !(categories.is_empty() && custom.is_empty()) {
        let outcome = crate::session::arm(&categories, &custom, minutes);
        if outcome.blocked {
            if let Some(lock) = crate::session::active_lock() {
                let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
                sessions::confirm_block(&conn, new_id, lock.locked_until)?;
            }
        }
        Some(outcome)
    } else {
        None
    };

    let enforced = block.as_ref().is_some_and(|outcome| outcome.blocked);
    crate::triggers::note_session_until(now + minutes * 60, enforced);

    crate::checkin::close(&app);
    crate::session::announce_ended(&app, session_id, "answered");
    crate::session::announce_started(&app, new_id);

    Ok(ContinueResult {
        session_id: new_id,
        block,
    })
}

/// "Start it again": reopen the ritual for what this session was about, every
/// option asked afresh - intention, prediction, length, what to quiet.
///
/// Express when the subject is known (the question was already answered once);
/// the full arrival otherwise, because a blank express screen asks nothing.
/// Does not record the check-in answer - the caller does that once the ritual
/// has actually opened, so a refusal (paused, session already running) leaves
/// the question on screen with the reason instead of eating both.
#[tauri::command(async)]
pub fn start_again(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    session_id: i64,
) -> Result<FocusResult, String> {
    let (intent, task_id) = {
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        let row = sessions::by_id(&conn, session_id)?
            .ok_or_else(|| format!("no session with id {session_id}"))?;
        // A task deleted while the question sat on screen must not be pointed
        // at: the title still carries the subject, the link would dangle.
        let task_id = row
            .task_id
            .filter(|id| tasks::is_open(&conn, *id).unwrap_or(false));
        (row.task_title, task_id)
    };

    let prefill = crate::popup::Prefill {
        express: intent.is_some(),
        intent,
        task_id,
    };

    match crate::triggers::request_focus(&app, prefill) {
        Ok(()) => Ok(FocusResult {
            opened: true,
            reason: None,
        }),
        Err(reason) => Ok(FocusResult {
            opened: false,
            reason: Some(reason),
        }),
    }
}

/// Start the record over: every session and every intention, gone. Tasks,
/// quotes and settings stay - this clears the mirror, not the desk.
///
/// Refused while a session is open. The banner and the check-in stand on
/// those rows, and a lock can outlive them; erasing the ground under a live
/// commitment would leave the block enforcing a session nothing remembers.
#[tauri::command]
pub fn clear_history(db: State<'_, Db>) -> Result<(), String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    let tx = conn
        .unchecked_transaction()
        .map_err(|err| format!("could not begin: {err}"))?;
    if !sessions::open_sessions(&tx)?.is_empty() {
        return Err(
            "A session is still open. End it - and answer its check-in - before starting over."
                .into(),
        );
    }
    db::clear_history(&tx)?;
    tx.commit().map_err(|err| format!("could not clear: {err}"))
}

/// Give a task a date: move it to the Schedule quadrant, joining the end.
#[tauri::command]
pub fn schedule_task(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    task_id: i64,
) -> Result<(), String> {
    let before = {
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        let before = schedule_state(&conn, task_id);
        tasks::append_to_quadrant(&conn, task_id, Some(false), Some(true))?;
        before
    };
    reconcile_schedule(&app, &db, task_id, before);
    Ok(())
}

// ---- Calendar commands -----------------------------------------------------

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalendarStatus {
    pub connected: bool,
    pub last_sync_ts: Option<i64>,
    pub last_sync_status: Option<String>,
}

#[tauri::command]
pub fn calendar_status(db: State<'_, Db>) -> Result<CalendarStatus, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    Ok(CalendarStatus {
        connected: db::get_setting(&conn, "google_connected").as_deref() == Some("1"),
        last_sync_ts: db::get_setting(&conn, "google_last_sync_ts").and_then(|s| s.parse().ok()),
        last_sync_status: db::get_setting(&conn, "google_last_sync_status"),
    })
}

/// Begin the OAuth flow. Blocking (browser + loopback), so run it off-thread
/// and report through the `calendar-status` event the flow emits.
#[tauri::command(async)]
pub fn google_connect(app: tauri::AppHandle) -> Result<(), String> {
    crate::calendar::oauth::connect(&app)
}

#[tauri::command]
pub fn google_disconnect(app: tauri::AppHandle) {
    crate::calendar::oauth::disconnect(&app);
}

#[tauri::command(async)]
pub fn calendar_sync_now(app: tauri::AppHandle) -> Result<String, String> {
    crate::calendar::sync::poll_once(&app)
}

/// Move a scheduled task's event. `start_ts` None = next free slot.
#[tauri::command(async)]
pub fn reschedule_task(
    app: tauri::AppHandle,
    task_id: i64,
    start_ts: Option<i64>,
) -> Result<i64, String> {
    crate::calendar::sync::reschedule(&app, task_id, start_ts)
}

/// Take a task off the calendar but leave it in Schedule.
#[tauri::command(async)]
pub fn unschedule_task(app: tauri::AppHandle, task_id: i64) -> Result<(), String> {
    crate::calendar::sync::remove_from_calendar(&app, task_id)
}

/// Open a URL in the default browser (the event's Google Calendar page).
#[tauri::command]
pub fn open_url(url: String) {
    crate::calendar::oauth::open_url(&url);
}

/// The wall, sent away early by a click. It would have left on its own; this
/// only shortens the wait, and records nothing - a wall you can wave off is
/// one you will not learn to resent.
#[tauri::command]
pub fn dismiss_wall(app: tauri::AppHandle) {
    crate::blocked::close(&app);
}

/// Closed without answering.
///
/// Recorded as its own outcome, never as a "no". A question you cannot decline
/// is a trap, and silence scored as failure would corrupt the one measurement
/// the check-in exists to make.
#[tauri::command]
pub fn dismiss_checkin(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    session_id: i64,
) -> Result<(), String> {
    {
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        sessions::mark(&conn, session_id, sessions::State::Unanswered, None)?;
    }
    crate::checkin::close(&app);
    crate::session::announce_ended(&app, session_id, "dismissed");
    Ok(())
}

/// Lift a block that is still owed time, and record that it happened.
///
/// `async` because it waits on the elevated helper for up to twelve seconds;
/// run on the main thread that would freeze every window, including the one
/// showing the button.
///
/// This is the only way out of a commitment before its time, and it exists
/// because the lock deliberately fails closed. It is not hidden and not
/// discouraged: the helper records it neutrally as a count, and an escape hatch
/// people feel judged for using is one they route around instead.
#[tauri::command(async)]
pub fn emergency_unblock() -> Result<(), String> {
    crate::session::emergency_unblock()
}

/// Re-register the elevated helper, prompting for administrator rights.
///
/// The one repair the user cannot perform from inside the app, surfaced as a
/// button rather than left as a command-line incantation.
#[tauri::command]
pub fn repair_helper() -> Result<(), String> {
    crate::elevate_register_helper()
}

#[tauri::command]
pub fn pause_status(db: State<'_, Db>) -> Result<Option<i64>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    Ok(crate::pause::paused_until(&conn))
}

#[tauri::command]
pub fn pause_for(db: State<'_, Db>, days: i64) -> Result<i64, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    crate::pause::pause_for_days(&conn, days)
}

#[tauri::command]
pub fn resume_now(db: State<'_, Db>) -> Result<(), String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    crate::pause::resume(&conn)
}

#[tauri::command]
pub fn get_settings(db: State<'_, Db>) -> Result<Vec<(String, String)>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    db::all_settings(&conn)
}

#[tauri::command]
pub fn set_setting(
    app: tauri::AppHandle,
    db: State<'_, Db>,
    key: String,
    value: String,
) -> Result<(), String> {
    {
        let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
        db::set_setting(&conn, &key, &value)?;
    }
    // The debounce reads its thresholds once at start; a change here reaches
    // it now rather than at the next launch. After the lock is released -
    // reload takes the same lock.
    if key == "unlock_threshold_sec" || key == "min_gap_sec" {
        crate::triggers::reload_thresholds(&app);
    }
    Ok(())
}

#[tauri::command]
pub fn recent_intentions(
    db: State<'_, Db>,
    limit: Option<i64>,
) -> Result<Vec<intentions::IntentionRow>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    intentions::recent(&conn, limit.unwrap_or(50))
}

/// Block-category toggles are remembered between rituals, because re-choosing
/// them every time is friction that buys nothing.
#[tauri::command]
pub fn remembered_categories(db: State<'_, Db>) -> Result<String, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    Ok(db::get_setting(&conn, "last_categories").unwrap_or_default())
}

#[tauri::command]
pub fn remember_categories(db: State<'_, Db>, categories: String) -> Result<(), String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    db::set_setting(&conn, "last_categories", &categories)
}

/// Sites the user added themselves, remembered the same way the toggles are.
///
/// Kept as a saved list rather than asked for each time: the three built-in
/// categories are somebody else's idea of distraction, and the ones you typed
/// are yours. Retyping them every session would be friction on exactly the part
/// that makes the blocklist worth having.
#[tauri::command]
pub fn remembered_sites(db: State<'_, Db>) -> Result<Vec<String>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    let saved = db::get_setting(&conn, "custom_sites").unwrap_or_default();
    Ok(split_sites(Some(&saved)).0)
}

#[tauri::command]
pub fn remember_sites(db: State<'_, Db>, sites: Vec<String>) -> Result<Vec<String>, String> {
    let (good, _) = split_sites(Some(&sites.join(",")));
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    db::set_setting(&conn, "custom_sites", &good.join(","))?;
    Ok(good)
}

/// Every quote, for the Settings list.
#[tauri::command]
pub fn list_quotes(db: State<'_, Db>) -> Result<Vec<db::quotes::Quote>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    db::quotes::all(&conn)
}

#[tauri::command]
pub fn add_quote(
    db: State<'_, Db>,
    text: String,
    author: Option<String>,
) -> Result<Vec<db::quotes::Quote>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    db::quotes::add(&conn, &text, author.as_deref())?;
    db::quotes::all(&conn)
}

#[tauri::command]
pub fn remove_quote(db: State<'_, Db>, id: i64) -> Result<Vec<db::quotes::Quote>, String> {
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    db::quotes::remove(&conn, id)?;
    db::quotes::all(&conn)
}

/// The quote to show on a surface right now, or nothing.
///
/// Resolved here rather than in the window so both surfaces get the same
/// shuffle rule, and so a ritual that opens with an empty reservoir simply has
/// no quote in it instead of a placeholder nobody chose.
#[tauri::command]
pub fn quote_for(db: State<'_, Db>, surface: String) -> Result<Option<db::quotes::Quote>, String> {
    let surface = db::quotes::Surface::parse(&surface)
        .ok_or_else(|| format!("unknown surface: {surface}"))?;
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    db::quotes::for_surface(&conn, surface)
}

/// Pin a quote to a surface, or pass `null` to shuffle.
#[tauri::command]
pub fn choose_quote(db: State<'_, Db>, surface: String, id: Option<i64>) -> Result<(), String> {
    let surface = db::quotes::Surface::parse(&surface)
        .ok_or_else(|| format!("unknown surface: {surface}"))?;
    let conn = db.0.lock().map_err(|_| "database lock poisoned")?;
    db::set_setting(
        &conn,
        surface.setting(),
        &id.map(|id| id.to_string()).unwrap_or_else(|| "shuffle".into()),
    )
}

/// Check one typed site without saving it, so the field can answer immediately.
///
/// Returns the tidied name, or the reason it cannot be blocked. The two live in
/// one command because the answer to "is this ok" is "here is what it becomes".
#[tauri::command]
pub fn check_site(site: String) -> Result<String, String> {
    let host = threshold_protocol::normalise_host(&site);
    threshold_protocol::valid_hostname(&host)?;
    Ok(host)
}

#[tauri::command]
pub fn dismiss_popup(app: tauri::AppHandle) -> Result<(), String> {
    crate::popup::close(&app).map_err(|err| err.to_string())
}

/// Where the database lives, for the dashboard's "your data is here" line.
#[tauri::command]
pub fn data_location() -> Result<String, String> {
    db::data_dir().map(|dir| dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn open_dashboard(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
