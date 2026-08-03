# Threshold

A local-only Windows desktop app that interrupts autopilot at the moment it
takes over: the first seconds after boot, wake, or unlock.

Instead of arriving at the desktop and finding yourself in a feed, Threshold
opens a fullscreen prompt that takes about twenty seconds — what you're here
for, whether you'll start it before opening anything else, what you'll do when
the urge to check something arrives, and how long you're committing. It can
then block distracting sites system-wide for that duration, across every
browser and every profile, and it keeps a small local record of how it's going.

It also holds a short to-do list, split by context and prioritised on an
Eisenhower matrix, so the prompt can show you what actually matters rather than
asking you to remember it.

## Why it works this way

Every interaction rule in this app traces to a specific finding, and several of
them are deliberately the opposite of what a habit app usually does:

- **The skip button is prominent and guilt-free.** In the "one sec" trial, the
  explicit dismiss option was the single most effective feature — more than the
  delay itself. Hiding it gets the app uninstalled.
- **Nothing celebrates setting an intention.** Praise for a stated intention
  licenses the behaviour you were avoiding. Rewards are reserved for sessions
  you finish.
- **The prompt changes daily.** Identical dialogs are habituated within a
  handful of exposures, so the look, the phrasing, and the interaction rotate.
- **Streaks forgive a miss.** Habit formation survives single misses; two
  consecutive ones reset it.
- **The escape hatch is official.** Users who defeat a tool with workarounds
  abandon self-regulation entirely, so there's a deliberate, costly, honest way
  out instead.

## Status

Early. Phase 0 (scaffold) is in place: Tauri v2 shell, tray app, and the
workspace for the elevated helper.

## Stack

Tauri v2 (Rust + WebView2), React, TypeScript, Tailwind, Motion. SQLite and
JSON on disk — no backend, no accounts, no telemetry, nothing leaves the
machine.

## Building

Requires Node, Rust (MSVC toolchain), and Visual Studio Build Tools.

```bash
npm install
npm run tauri dev
```
