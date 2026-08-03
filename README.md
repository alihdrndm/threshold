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
you what actually matters rather than asking you to remember it.

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
- **The escape hatch is official.** Users who defeat a tool with workarounds
  abandon self-regulation entirely, so there's a deliberate, costly, honest way
  out instead of one you have to invent.

The questions themselves never rotate. Each carries a specific effect, and
rewording them into something friendlier would quietly throw that away.

## How the blocking works

Entries go in the hosts file, between markers, pointing at `0.0.0.0` — an
instant connection failure rather than a slow timeout, and never a redirect to a
local server, because the social domains are HSTS-preloaded and that produces
certificate warnings instead of a clean stop.

That alone is not enough: Chrome and Edge resolve names over HTTPS and ignore
the hosts file entirely. So while a block is armed, Threshold turns DNS-over-HTTPS
off by policy. **Your browsers will say they are "managed by your organization"
during a session.** That is this, and it is removed the moment the block lifts.

The commitment lock lives in an admin-only directory, so the app can't lift a
block early and neither can you by editing a file. It records when it was armed
twice — wall-clock and milliseconds since boot — because with only a wall clock,
winding the system clock back makes elapsed time unknowable, and a lock that
can't measure time either frees instantly or holds forever. A lock it can't read
is treated as still running: damaging the file must not be a way out.

Nothing is ever a jail. The emergency exit always works, and every use is
recorded as a plain count with no shame attached.

## Status

Phases 0–7 are built: scaffold, triggers, ritual with local history, design pass,
blocking engine, tasks and matrix, pause and settings, installer.

Not yet done: a clean-VM install/uninstall run, and per-browser confirmation by
hand. See [TESTING.md](TESTING.md) for exactly what is and isn't verified.

## Stack

Tauri v2 (Rust + WebView2), React, TypeScript, Tailwind, Motion, dnd-kit,
SQLite. No backend, no accounts, no telemetry. Nothing leaves the machine.

Idle footprint is about 28 MB — both windows are created on demand, because a
declared window boots a whole WebView2 instance for a dashboard most days never
opened.

## Building

Requires Node, Rust (MSVC toolchain), and Visual Studio Build Tools.

```bash
npm install
npm run tauri dev        # develop
npx tauri build          # installer at target/release/bundle/nsis
```
