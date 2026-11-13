# crastu-audio

Capture seam for dictation and transcribe-file. No UI, no hotkey, no speech-to-text.

- `AudioSource`: mono `f32` samples at the source's own rate. The WAV `FileSource` is in the sibling crate `crastu-audio-wav` (PCM 8/16/24/32-bit, float 32/64, any channel count, rates 1..=768000, files up to 1 GiB). `SyntheticSource` (silence, sine, noise) and, with the `mic` feature, `MicSource` (cpal; default device or one by name).
- `Converter`: downmix, resample to 16 kHz (windowed sinc), 20 ms `Frame`s with a `Level { peak_db, rms_db }` and a clipping flag.
- `ring_buffer`: bounded SPSC buffer for the capture callback; the producer never blocks and counts `dropped_frames`.
- `VoiceGate` / `EnergyGate`: speech start and end with hysteresis and a hangover. Silero VAD can replace it behind the trait.

Without `mic` the crate is safe code that compiles for iOS, Android and Linux. `AudioError::PermissionDenied` and `PermissionUnknown` carry the status words of `crastu-os-perms`; the caller maps them.

Live check (needs a microphone and, on macOS, the permission for the terminal):

```sh
cargo run -p crastu-audio --example level --features mic
```
