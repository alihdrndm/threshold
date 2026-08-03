# Testing

Automated checks cover the logic. The trigger moments themselves need a human,
because no harness can genuinely reboot, sleep, or unlock the machine.

```bash
cargo test                  # 65 tests across both binaries
cargo clippy --all-targets
npm run check               # tsc + eslint
```

## How the UI is tested

Driven by keyboard rather than simulated mouse clicks. Display scaling corrupts
screen coordinates on multi-monitor Windows, and a coordinate-based test can be
fooled by a scaling bug into "clicking" the wrong control; keyboard focus
cannot. Verification is by screenshot plus a state assertion, never by one alone
— an early pass reported the app healthy while every window was in fact showing
a connection error.

## Verified automatically

### Lifecycle
| Behaviour | How |
|---|---|
| Starts to tray with no visible window | Win32 window enumeration + `IsWindowVisible` |
| Second launch focuses the first | process count stays 1, window becomes visible |
| Closing hides rather than exits | real `WM_CLOSE`; process survives |
| Idle footprint | 28.5 MB across the process tree (budget: 60 MB) |
| Installer size | 1.78 MB (budget: 15 MB) |

### Triggers
| Behaviour | How |
|---|---|
| Hidden trigger window created, never shown | class `ThresholdTriggerWindow`, not visible |
| Wake or unlock opens the ritual | fullscreen window appears on the primary display only |
| Second trigger inside 15 min suppressed | 0 popups; log shows `suppressed (TooSoon)` |
| Scheduled tasks register without elevation | `--register-tasks` then `--task-status` |
| Relaunch path | `schtasks /run /tn ThresholdUnlock` starts the app and shows the ritual |

### Ritual
| Behaviour | How |
|---|---|
| All six screens render | screenshot per screen |
| Prediction cannot be skipped | Enter from the intention field lands on the question, not past it |
| Answers are the user's own | chose No and 50 min; both stored as chosen, not defaulted |
| Completed path records fully | outcome, trigger, prediction, duration, categories, text |
| Honourable exit records differently | `browsing`, no duration, no categories, no text |
| Chips come from Do First | a matrix task appears as the first chip |
| Block toggles remembered | still selected on the next ritual |
| Themes rotate | all four previewed via `--theme=` |

### Blocking (run against the real system, then reverted)
| Behaviour | How |
|---|---|
| Dry run changes nothing | hosts and registry byte-checked before and after |
| Block applies | hosts 20 → 55 lines; four registry values set |
| Sites actually blocked | blocked domains fail to resolve; others unaffected |
| Lock is tamper-proof | non-elevated write and delete both denied |
| Early unblock refused | block survives; "25 minutes still committed" |
| Corrupt lock refused | fails closed |
| Emergency exit always works | succeeds even with a corrupt lock; drift recorded |
| Natural expiry | normal unblock succeeds once the time is served |
| hosts restored | byte-identical SHA256, CRLF preserved |
| Zero registry residue | keys and empty parents removed; `SOFTWARE\Policies` intact |

### Tasks
| Behaviour | How |
|---|---|
| New tasks land in the Inbox unclassified | both flags null |
| Drag between quadrants persists | moved, restarted, still there |
| Drag works by keyboard | space to lift, arrows to move |
| Do First soft cap | a fourth task shows the note, and still allows it |

## Needs a person

Register the relaunch tasks first (`threshold.exe --register-tasks`). The
15-minute debounce spans all of these, so leave gaps or you will be testing the
debounce instead of the trigger.

1. **Cold boot.** Reboot. The ritual appears about ten seconds after the
   desktop, exactly once.
2. **Sleep and wake.** Sleep, wait, wake. Appears once. On a Modern Standby
   laptop the wake may arrive only as an unlock — either is fine, twice is not.
3. **Unlock after a long absence.** `Win+L`, wait past twenty minutes, unlock.
   Appears.
4. **Unlock after a brief absence.** `Win+L`, unlock within a minute. Nothing
   happens. This is the rule that stops it becoming a nuisance.
5. **Tray icon and menu.** Appearance, and the pause submenu.
6. **Motion.** Whether the step transitions and the glow feel right. Stills
   cannot show timing.

Remove the tasks again with `threshold.exe --unregister-tasks`.

## Not yet verified

- **Fresh-machine install and uninstall.** The installer builds and the
  uninstall hooks are written, but "install on a clean VM, use, uninstall, leave
  zero residue" has not been run — there is no VM here. The individual cleanup
  steps are verified against the live system; the packaged sequence is not.
- **Real browser behaviour.** Blocking is verified at the DNS resolver, which is
  what the hosts file governs. Chrome, Edge and Firefox were not each opened by
  hand to confirm the page fails and that Secure DNS reads as managed-off.

## Running the app directly

A plain `cargo build` binary loads the frontend from the Vite dev server, so
launching `target\debug\threshold.exe` on its own shows a connection error
instead of the UI. Use one of:

```bash
npm run tauri dev                    # dev server + app
npx tauri build --debug --no-bundle  # standalone exe with the UI embedded
```

## Maintenance commands

```
threshold.exe --recent              # print recorded history
threshold.exe --task-status         # are the relaunch tasks registered
threshold.exe --register-tasks      # register them (no elevation)
threshold.exe --unregister-tasks    # remove them
threshold.exe --register-helper     # the one elevated task (UAC)
threshold.exe --unregister-helper   # remove it
threshold.exe --theme=ember         # preview a rotating theme
```
