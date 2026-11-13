# crastu-loudness

ITU-R BS.1770-4 integrated loudness: K-weighting for any sample rate (checked against the published 48 kHz coefficients), 400 ms blocks at a 100 ms hop, the absolute gate at -70 LUFS and the relative gate 10 LU under the gated mean. `LoudnessMeter` streams, `measure_lufs` takes a whole buffer, `gain_to_target(measured, SOCIAL_TARGET_LUFS)` gives the linear gain to the preset target. Mono is measured as one channel, stereo as the front pair; surround channel weights are not implemented.

Split from `crastu-mix` (which re-exports it) to keep both crates under the 2,000-line limit. Std only, safe code; compiles for iOS and Android. Non-finite samples are treated as silence.
