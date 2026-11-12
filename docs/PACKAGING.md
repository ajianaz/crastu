# Packaging (macOS app bundle)

A bare binary gets its permissions attributed to the terminal that started it, so permission and hotkey tests need a real `.app` with an `Info.plist`. `cargo xtask bundle` builds one.

## Build the bundle

```sh
cargo xtask bundle            # release build
cargo xtask bundle --debug    # debug build, faster for quick tests
cargo xtask bundle --sign "<identity>"   # sign with a real identity instead of ad-hoc
```

The command runs `cargo build -p crastu-desktop` and writes `target/bundle/Crastu.app`:

- `Contents/MacOS/crastu`: the binary.
- `Contents/Info.plist`: generated. Version comes from the workspace version. It declares `NSMicrophoneUsageDescription`; Accessibility and Input Monitoring have no plist keys.
- `Contents/MacOS/*.dylib`: every `*.dylib` in the folder named by `CRASTU_LIBS_DIR`, when set (decision 28: ONNX Runtime and sherpa libraries sit next to the executable). When it is not set, the command prints that no libraries were bundled.

It then signs the bundle (`codesign --force --deep --sign -`) and checks it with `codesign --verify --deep --strict`. It never notarises.

Placeholders to confirm before a real signing identity is used: the bundle identifier `dev.crastu.desktop` and the minimum system version `13.0` (a floor of our own; no better figure for GPUI and Metal was found).

## Ad-hoc signing

The default signature is ad-hoc (`-`): no Apple identity is involved. Its code requirement is tied to the exact binary, so it changes with every build, and macOS may treat each build as a new app for permissions (the old grants stop applying and the prompts return). A stable signing identity fixes that; choosing one is the owner's decision.

## Hands-on checklist (issues #56 and #4)

Needs a person at the Mac; the build and `--smoke` run need none.

1. `cargo xtask bundle --debug`, then `target/bundle/Crastu.app/Contents/MacOS/crastu --smoke` must exit 0 (it never prompts).
2. `tccutil reset Microphone dev.crastu.desktop` (and `tccutil reset Accessibility dev.crastu.desktop`, `tccutil reset ListenEvent dev.crastu.desktop` for Input Monitoring) to start clean. After each rebuild with the ad-hoc signature, reset again.
3. `open target/bundle/Crastu.app`.
4. #56: in onboarding, check that each permission row shows its real status, that the Microphone request shows the macOS prompt with the usage text above, that the Accessibility and Input Monitoring buttons open the right Settings pane, and that the status updates after a grant without restarting.
5. #4: with the permissions granted, check that the hold-to-talk and toggle hotkeys fire (including a modifier-only one), that the text is pasted into the previously focused app, and that the clipboard is restored afterwards.
6. Note anything that differs from a run started from the terminal.
