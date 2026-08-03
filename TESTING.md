# Testing

Automated checks cover the logic; the trigger moments themselves need a human,
because no test harness can genuinely reboot, sleep, or unlock the machine.

## Automated

```bash
cargo test          # debounce rules
cargo clippy --all-targets
npm run check       # tsc + eslint
```

## Phase 1 — verified automatically

| Behaviour | Method |
|---|---|
| Hidden trigger window is created | `ThresholdTriggerWindow` present, never visible |
| Wake trigger opens the ritual | fullscreen window appears |
| Second trigger inside 15 min is suppressed | popup does not reappear; log shows `suppressed (TooSoon)` |
| Ritual covers the primary display only | secondary monitor untouched |
| Scheduled tasks register without elevation | `--register-tasks` then `--task-status` |
| Task relaunch path works | `schtasks /run /tn ThresholdUnlock` starts the app and shows the ritual |
| Ritual closes cleanly | window closes, app stays resident |

## Phase 1 — needs a person

Register the tasks first (`threshold.exe --register-tasks`), and note that the
15-minute debounce spans all of these: leave a gap between tests or you will be
testing the debounce instead of the trigger.

1. **Cold boot / logon.** Reboot. Expect: the ritual appears roughly 10 seconds
   after reaching the desktop. Exactly once.
2. **Sleep and wake.** Sleep the machine, wait a moment, wake it. Expect: the
   ritual appears once. On a Modern Standby laptop the wake may arrive only as
   an unlock — either path is fine, but it must not appear twice.
3. **Unlock after a long absence.** `Win+L`, wait more than 20 minutes, unlock.
   Expect: the ritual appears.
4. **Unlock after a brief absence.** `Win+L`, unlock within a minute or two.
   Expect: nothing. This is the rule that stops it becoming a nuisance.
5. **Re-trigger inside the window.** Any trigger within 15 minutes of the last
   ritual. Expect: nothing.

Watching what it decided, while running from a terminal:

```
triggers: Unlock suppressed (TooSoon)
```

To remove the tasks again:

```
threshold.exe --unregister-tasks
```

## Note on running the app directly

A plain `cargo build` binary loads the frontend from the Vite dev server, so
launching `target\debug\threshold.exe` on its own shows a connection error
instead of the UI. Use one of:

```bash
npm run tauri dev                    # dev server + app
npx tauri build --debug --no-bundle  # standalone exe with the UI embedded
```
