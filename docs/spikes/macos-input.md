# Spike: macOS push-to-talk hotkey and paste into the focused app

Issue #4. **Status: in progress.** The permission and clipboard parts are measured; the hotkey and the paste into other apps still need a hands-on run. Code: [`spikes/macos-input`](../../spikes/macos-input) (standalone crate, not part of the workspace).

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

## Not measured yet

- Hotkey: does the tap see Right Option, Right Command, Fn/Globe and F13 on this keyboard, with and without swallowing the key; does a key-up get lost when the tap is disabled by the system; comparison with the `global-hotkey` crate.
- Paste into VS Code, Safari, Terminal, Notes, Mail and Zed: whether Cmd+V is accepted, how long until the target reads a promised item, whether the restore races with a slow reader.
- Secure input (password fields): what happens to a synthetic paste and to the tap.
- The typing fallback: speed, and apps that drop synthetic Unicode events.
- Focus changes between key-down and insertion.

## Next

Run the hands-on session (one grant, no rebuild in between), fill in the tables above, and turn the numbers into a recommendation for `crastu-os-input` (traits `HotkeySource` and `TextSink`, macOS adapter, onboarding steps for the three permissions).

## Reproduce

```sh
cd spikes/macos-input
cargo test --release                    # logic tests, no permission
./target/release/spike-macos-input selftest
./bundle.sh                             # builds CrastuInputSpike.app, ad-hoc signed
open -n target/CrastuInputSpike.app --args check --prompt
open -n target/CrastuInputSpike.app --args run --key rightoption --sink paste-promise --minutes 10
tail -f ~/Library/Logs/crastu-spike-input.log
```
