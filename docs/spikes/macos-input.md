# Spike: macOS push-to-talk hotkey and paste into the focused app

Issue #4. **Status: in progress.** Permissions, clipboard, hotkey, paste and typing are measured, hands-on, on one Mac; the secure-input check, the focus-change check and the dictation pill are still to do, and Fn needs another keyboard. Code: [`spikes/macos-input`](../../spikes/macos-input) (standalone crate, not part of the workspace).

## Question

Dictation needs a global hold-to-talk key (including modifier-only keys and Fn) and a way to put text into whatever app has focus without losing what the user had on the clipboard. Which APIs, which permissions, and what breaks?

## What the spike does

A windowless agent app (`bundle.sh` wraps the binary as `CrastuInputSpike.app`, `LSUIElement`) so that permissions belong to the spike itself, as they will to Crastu.app.

| Part | Implementation |
|---|---|
| Hotkey | `CGEventTapCreate` on the session, keyboard events plus `flagsChanged`, behind a `HotkeySource` trait. Right Option, Right Command, Fn and F13 are supported; the right-hand side is told from the left through the device bits of the event flags. A key used as a modifier in a shortcut, a hold shorter than 200 ms and key repeat are ignored. |
| Focus | `NSWorkspace.frontmostApplication` when the key goes down; the target is activated again if focus moved before the text is inserted. |
| Insert | A `TextSink` trait with three paths: `paste` (write the string, post Cmd+V, restore after a fixed delay), `paste-promise` (the pasteboard item is promised, so the moment the target reads it is observed and the clipboard is restored right after), and `type` (Unicode keyboard events in chunks of 20 UTF-16 units). |
| Clipboard | Snapshot of every item and every type, restored only if nobody else changed the clipboard in between (change count compared). |
| Self-test | `selftest` exercises the clipboard parts without any permission and puts the user's clipboard back. |
| Dictation pill | `pill` shows a small floating panel (a borderless, non-activating `NSPanel` with five bars animated by Core Animation) while the user keeps typing in another app. It samples the frontmost app before, during and after, and says whether focus was ever taken. `--busy` blocks the main thread for 150 ms every second to see whether the animation keeps running. `--activating` is a control: an ordinary key window that must steal focus, so the measurement is shown to detect it. |

## Measured so far

### Permissions

Three separate checks, all `false` for a fresh app:

| Need | Check | Request |
|---|---|---|
| Accessibility | `AXIsProcessTrusted` | `AXIsProcessTrustedWithOptions` with `AXTrustedCheckOptionPrompt` |
| Input Monitoring (listen to keys) | `CGPreflightListenEventAccess` | `CGRequestListenEventAccess` |
| Post synthetic events | `CGPreflightPostEventAccess` | `CGRequestPostEventAccess` |

The checks answer immediately; the system dialog is asynchronous, so a status read right after the request is still `false`. After the grant, a newly started process reported `true` for all three (an already running process was not tested).

### Ad-hoc signing breaks the permission on every rebuild

The first grant did not take effect. The unified log of `tccd` showed `Failed to match existing code requirement for subject dev.crastu.spike.input and service kTCCServiceAccessibility`, with two different `cdhash` values: the existing entry belonged to an earlier build of the same app, the running one was newer. The log lists `cdhash` values, which indicates that for an ad-hoc signature the requirement is the binary's hash, so every build is a new app to macOS and loses the user's grant. `tccutil reset <Service> <bundle id>` clears the stale entries.

Consequences for Crastu: release builds must be signed with a Developer ID certificate (the requirement then names the signing identity and the bundle id, not the hash), otherwise every update would send users back through the permission steps. During development, do not rebuild between granting and testing.

Also observed: a permission granted to the terminal app that launches a command does not apply to the spike; the spike runs as its own app and needs its own entries.

### Clipboard snapshot, restore and promised data (`selftest`, no permission needed)

| Check | Result |
|---|---|
| Two items with five types (text, RTF, PNG, custom type) written and read back | pass; macOS adds a sixth type, `public.utf16-external-plain-text`, derived from the UTF-8 text |
| Overwrite, then restore from the snapshot, then compare every item and type byte for byte | pass (2 items, 6 types, 4185 bytes) |
| The comparison reports a difference when the clipboard really differs (negative control) | pass |
| The change count moves on every write | pass |
| A promised item is not produced when written (provider calls 0) and is produced on the first read (1 call), with the right text | pass |
| The user's own clipboard, saved first, is identical after the test | pass (run twice with different content) |
| A paste that fails after the clipboard was cleared (the shortcut cannot be sent) puts the clipboard back, plain and promised variant | pass; without the restore guard both fail, so the test detects the bug it was written for |

### Hotkey events (raw trace, `CGEventTap` at the HID, session and annotated-session locations)

Keyboard: a generic "USB Keyboard" (not an Apple keyboard). Every key below arrived identically at all three tap locations, as `flagsChanged` events with `sourceState` 1 (real hardware):

| Key | Keycode | Flags down | Flags up |
|---|---|---|---|
| Right Option | 61 | `0x00080140` (alternate `0x80000` + device bit `0x40`) | `0x00000100` |
| Left Option | 58 | `0x00080120` (device bit `0x20`) | `0x00000100` |
| Right Command | 54 | `0x00100110` (device bit `0x10`) | `0x00000100` |
| Right Shift | 60 | `0x00020104` (device bit `0x04`) | `0x00000100` |

So the right-hand modifier is told from the left through the device bits of the flags, as the spike does, and `0x100` is always set. The hold-to-talk key is detected from `flagsChanged`: down when the device bit is set, up when it clears.

### Paste into other apps (`paste-promise`, launched from a terminal)

Eleven insertions plus two in Notes, hold on Right Option, text inserted into the app that was in front when the key went down. "Read after" is the time from sending Cmd+V until the target asked for the promised data.

| App | Insertions | Read after (ms) | Clipboard restored identical |
|---|---|---|---|
| VS Code | 4 | 25, 47, 64, 80 | 4 of 4 |
| Safari | 3 | 8, 16, 66 | 3 of 3 |
| Terminal | 1 | 18 | 1 of 1 |
| Thunderbird | 1 | 83 | 1 of 1 |
| Zed | 1 | 4 | 1 of 1 |
| Termul | 1 | 30 | 1 of 1 |
| Notes | 2 | 50, 20 | 1 of 2 |

The target always read the promise (so the clipboard could be restored right after, not after a fixed delay); no run fell back to typing; the frontmost app never changed; total time per insertion was 62 to 195 ms, 50 ms of it a deliberate wait for other representations. Two holds of 59 and 119 ms were ignored as taps. The one restore that was not identical was the first in Notes, from a clipboard of four types and 1,336 bytes; the same clipboard restored identically the second time. Not explained (a type that changes when read again is the likely cause).

### Typing (`type`, launched from a terminal)

Four insertions (VS Code three times, Safari once; Terminal was not tried in this mode). The text, with accents, Japanese and emoji in four UTF-16 chunks of at most 20 units, arrived intact each time, 14 to 45 ms in total.

### A bundle launched with `open` received no events

The same binary behaved differently by how it was started. Wrapped as `CrastuInputSpike.app` (an `LSUIElement` agent with its own permission entries, all three reported granted) and started with `open`, the tap was created without error but no key event was ever logged, in sessions of 12 and 15 minutes in which the user reports pressing the key. Started from a terminal (the terminal app, which already had Input Monitoring, as the responsible process), the first press worked. A separate Swift tracer run from the terminal received events at once. Not root-caused. Two untested suspects: the process never starts an `NSApplication` event loop (the pill mode does and was not tried from the bundle), and how the permission is attributed to a bundle. Testing either needs a rebuild, which with an ad-hoc signature removes the user's grants, so it waits for a stable signing identity. For Crastu: the first thing to check once the shell exists is that the hotkey works from the signed, installed app, not only from a development run.

## Limits of the spike that the real implementation has to handle

Found by an independent review of the spike (Cora agent-as-reviewer, 2026-11-11). The first two are fixed in the spike; the others are noted here for `crastu-os-input`.

- **Every exit path restores the clipboard.** The first version returned early after clearing the clipboard when sending the shortcut failed, leaving it empty and the promised item pointing at a dropped provider. Now one guard restores it (unless someone else changed it since) and the provider outlives the restore.
- **Typed text starts from no modifiers.** Unicode key events no longer inherit a modifier the user still holds.
- **Unreadable types.** A pasteboard type that lists but gives no data cannot be restored. The snapshot now counts them and says so in its summary; the restore is lossy for those.
- **A late paste.** If the target takes longer than the one-second wait to process Cmd+V, the clipboard has been restored by then and the typing fallback has run: the late paste inserts the old clipboard on top of the typed text. A real implementation needs a way to tell "slow" from "ignored" (for example confirming the insertion through Accessibility) before it types.
- **Blocking pasteboard reads next to an active tap.** Snapshotting reads data from the pasteboard on the main thread, which also serves the event tap. A slow lazy provider in another app can stall the tap, and the system disables a tap that does not answer in time (the spike re-enables it). The tap should run on its own thread, with the pasteboard work elsewhere.
- **Our own events pass through the tap.** The synthetic Cmd+V and Unicode events are seen by the tap while an insertion runs; a hotkey pressed again during an insertion marks that hold as a shortcut and ignores it. Harmless in the spike; the real tap should recognise its own events.
- **The provider and the restore.** The provider is dropped after the restore. If another app changed the clipboard first, our promised item is already gone from the pasteboard, so nothing points at the dropped provider.
- **Panics.** The tap callback is `extern "C-unwind"`; a panic must not unwind through CoreGraphics, so it is caught and the event passes through.

## Not measured yet

- Fn/Globe (this keyboard has none) and F13; whether a key-up is lost when the system disables the tap; comparison with the `global-hotkey` crate.
- The bundle problem above.
- Secure input (password fields): what happens to a synthetic paste and to the tap.
- The typing fallback in Terminal, Notes and Zed, and apps that drop synthetic Unicode events.
- Focus changes between key-down and insertion.
- The dictation pill: the code is written and compiled, not run yet. Still to do by hand: the control first (it must take focus), then the non-activating panel while typing in VS Code and Safari, with and without `--busy`, and the same window made with GPUI (`WindowKind::PopUp`, no focus) to decide whether the pill needs a small native helper.

## Next

Finish the hands-on checks above (secure input, a focus change during a hold, the pill), then turn the numbers into a recommendation for `crastu-os-input` (traits `HotkeySource` and `TextSink`, macOS adapter, onboarding steps for the three permissions) and settle the bundle problem with a stable signing identity.

## Reproduce

```sh
cd spikes/macos-input
cargo test --release                    # logic tests, no permission
./target/release/spike-macos-input selftest
./bundle.sh                             # builds CrastuInputSpike.app, ad-hoc signed
open -n target/CrastuInputSpike.app --args check --prompt
# The hotkey works when the binary is started from a terminal that has Input Monitoring and
# Accessibility (see "A bundle launched with `open` received no events"):
target/CrastuInputSpike.app/Contents/MacOS/spike-macos-input run --key rightoption --sink paste-promise --minutes 10
target/CrastuInputSpike.app/Contents/MacOS/spike-macos-input run --key rightoption --sink type --minutes 5
target/CrastuInputSpike.app/Contents/MacOS/spike-macos-input pill --activating --seconds 6   # control: must take focus
target/CrastuInputSpike.app/Contents/MacOS/spike-macos-input pill --seconds 10 --busy
tail -f ~/Library/Logs/crastu-spike-input.log
```
