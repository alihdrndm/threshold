# Threshold

A local-only Windows app that interrupts autopilot at the moment it takes over:
the first seconds after boot, wake, or unlock.

Instead of arriving at the desktop and finding yourself in a feed, Threshold
opens a fullscreen prompt that takes about twenty seconds — what you're here
for, whether you'll start it before opening anything else, what you'll do when
the urge to check something arrives, and how long you're committing. It can then
block distracting sites system-wide for that duration, across every browser and
every profile, and it keeps a small local record of how it's going.

It also holds a short to-do list on an Eisenhower matrix, so the prompt can show
you what actually matters rather than asking you to remember it — and so you can
start a focus session on one of those tasks directly.

## Why it works this way

Every interaction rule traces to a specific finding, and several are deliberately
the opposite of what a habit app usually does:

- **The skip button is prominent and guilt-free.** In the "one sec" trial the
  explicit dismiss option was the single most effective feature — more than the
  delay itself. Hiding it gets the app uninstalled.
- **Nothing celebrates setting an intention.** Praise at that moment licenses the
  behaviour you were avoiding. Reward is reserved for sessions you finish.
- **The prompt changes daily.** Identical dialogs are habituated within a handful
  of exposures, so the palette, the phrasing, and even how you answer rotate
  across four themes: some days you type your intention, some days you pick it.
- **Streaks forgive a miss.** Habit formation survives single gaps; two
  consecutive misses reset it.
- **Walking away is not scored as failure.** "I didn't answer" and "I answered
  no" are stored as different things. Counting silence as a lapse would corrupt
  the only self-report the app collects, and would punish you for stepping away
  from your desk.
- **No number invites a target.** Counts, not percentages; no progress bars, no
  colour-coded goals. A target turns a mirror into a scoreboard, and this is
  meant to be a mirror.
- **The escape hatch is official.** Users who defeat a tool with workarounds
  abandon self-regulation entirely, so there's a deliberate, costly, honest way
  out instead of one you have to invent.

The questions themselves never rotate. Each carries a specific effect, and
rewording them into something friendlier would quietly throw that away.

## Focus sessions

Clicking **Focus** on a task starts a session: a commitment to that one task for
a set time. It's one screen, not the full ritual — you already decided by
picking the task, so the only open questions are what you'll do when the urge
arrives, how long, and what to quiet.

The prediction is still asked rather than assumed. It's one tap, and it's the
thing the closing question is scored against; a default would mean scoring
against an answer nobody gave.

While a session runs, the dashboard says so — a strip naming the task with
roughly how long is left, and that task's card marked in the matrix. The task is
named and the blocklist isn't: foregrounding what you're avoiding makes it more
available, not less. Minutes, never seconds, and no progress bar — a
second-by-second countdown in the corner of the screen is exactly the kind of
attention magnet this app exists to remove.

**A session doesn't require blocking.** Committing to a task for a while is
worth something on its own, and choosing no categories used to produce a ritual
and then nothing at all. But only a session with a confirmed block silences the
boot, wake and unlock prompts: a block leaves physical evidence it's still in
force, whereas a block-less session is a self-report, and the most likely state
at a wake forty minutes in is that you left and came back — which is precisely
what the prompt is for.

### The closing question

When a session ends, a small window in the corner asks how it went: *"25 minutes
on create stripe account. You thought you would."* — Did it / Partly / Not this
time, with an option to tick the task off.

It's small and in the corner on purpose. The ritual earns the whole screen
because it stands at a threshold; a check-in is a report on something already
finished, and taking over the screen at the *end* of work is punishment for
finishing, which teaches people never to let a session end cleanly.

Three answers rather than two, because a binary forces a partly-true session
into one of two lies and the lie people pick is the harsh one. Nothing happens
after you answer — reassurance would imply there was something to be reassured
about. Closing it without answering is a first-class outcome, and it's never
recorded as a "no".

If the machine was asleep when the session ended, the question is dropped rather
than asked hours late. An answer reconstructed from memory, scored against a
real prediction, is worse than no answer at all.

That's what makes the **Follow-through** number on the Overview possible: every
ritual since the first has asked whether you'd follow through, and until the
check-in existed nothing ever compared the answer to what happened. It reads
*"7 of 12, of the times you said you would"* — only answered sessions counted,
and only the optimistic predictions in the denominator. The useful response to a
low number is to commit to less, not to try harder.

## How the blocking works

Entries go in the hosts file, between markers, pointing at a loopback address
Threshold owns. The site fails instantly with a plain connection error — never a
certificate warning, because Threshold closes the connection without ever
presenting one — and the attempt itself is observable: the app sees it arrive,
reads which site was asked for from the handshake, and can answer with a quote
you chose. Blocking by a null address could only ever refuse an urge; this one
gets to reply to it.

That alone is not enough: Chromium browsers and Firefox resolve names over HTTPS
and ignore the hosts file entirely. So while a block is armed, Threshold turns
DNS-over-HTTPS off by policy — Chrome, Edge, Brave, Vivaldi and Firefox. **Your
browsers will say they are "managed by your organization" during a session.**
That is this, and it is removed the moment the block lifts.

One caveat, stated rather than hidden: browsers remember addresses briefly, so a
site visited moments before the block may keep working for up to a minute, and
the reverse after it lifts. That memory belongs to the browser; a reload sorts
it out.

The commitment lock lives in an admin-only directory, so the app can't lift a
block early and neither can you by editing a file. It records when it was armed
twice — wall-clock and milliseconds since boot — because with only a wall clock,
winding the system clock back makes elapsed time unknowable, and a lock that
can't measure time either frees instantly or holds forever. A lock it can't read
is treated as still running: damaging the file must not be a way out.

Because the block outranks the app, the two can disagree, and the app is built to
say so rather than paper over it. A session ends on schedule even when the helper
still refuses to unblock; the banner and the check-in then tell you the sites
stay quiet a little longer, instead of announcing you're free while nothing
loads.

Nothing is ever a jail. The emergency exit always works — it's in Settings under
"If you need out" — and every use is recorded as a plain count with no shame
attached.

## Appearance

Light, dark, or whatever Windows is doing. The preference is applied before the
first paint, so there's no flash of the wrong scheme on open.

The ritual stays dark regardless. A white fullscreen window at boot is not a
kindness, and its four rotating palettes are dark by design — that's structural
rather than a promise: a document carrying a rotating theme resets to the dark
palette wholesale, so the appearance setting cannot reach it.

The Eisenhower quadrants carry colour by one rule: a zone's prominence is its
distance from the page — upward in lightness on dark, downward on light — so the
ordering reads the same in both. Hue carries identity, distance carries
priority. Nothing is red: alarm colour just teaches you to stop seeing it.

## Status

0.4.0. Phases 0–7 are built — scaffold, triggers, ritual with local history,
design pass, blocking engine, tasks and matrix, pause and settings, installer —
plus appearance options, focus sessions, the closing check-in, prediction
calibration, sites of your own alongside the built-in categories, and a quote
reservoir: lines you keep, shown on the ritual's opening screen and in a small
corner window when a blocked site refuses.

Not yet done: a clean-VM install/uninstall run, and per-browser confirmation by
hand. See [TESTING.md](TESTING.md) for what is and isn't verified — note its
test counts predate this release.

## Stack

Tauri v2 (Rust + WebView2), React, TypeScript, Tailwind, Motion, dnd-kit,
SQLite. No backend, no accounts, no telemetry. Nothing leaves the machine.

Sitting in the tray with no window ever opened costs about 14 MB. Every window
is created on demand, because a window declared in the config boots a whole
WebView2 instance at launch for a dashboard most days never opened.

Open the dashboard once, though, and it settles around 150 MB and stays there:
closing a window hides it rather than destroying it, so its WebView2 stays
resident until you quit. That's the honest number for a day you actually looked
at it, and it's the obvious thing to improve next.

## Building

Requires Node, Rust (MSVC toolchain), and Visual Studio Build Tools.

```bash
npm install
npm run tauri dev        # develop
npm run check            # tsc, eslint, colour-token guard, vitest
cargo test --workspace   # from src-tauri
npx tauri build          # installer at target/release/bundle/nsis
```

`npm run check` includes a guard that fails on a hardcoded colour. Every colour
resolves through a CSS variable so one attribute can flip the whole interface;
a literal survives the flip and quietly ruins exactly one scheme — the one
nobody was looking at.
