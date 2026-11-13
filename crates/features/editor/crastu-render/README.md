# crastu-render

Deterministic motion and caption composition (issue #35, slice 1). No GPU, fonts, wall clock or UI dependency; safe code that compiles for iOS and Android.

- `motion` (the crate `crastu-motion`, re-exported): `Easing` (linear, cubic in/out/in-out, back) and `Spring` (duration and bounce, closed form), both evaluated at an elapsed time, never stepped.
- `Composition { width, height, fps, duration, layers }`; layers are solid, image and video references, `TitleLayer` and `CaptionLayer { words, style }`. `CaptionStyle` is serde data with a version (`CaptionStyle::from_json` rejects newer versions and unknown fields): word highlight, pop-in and karaoke fill derived from `WordTiming`s, shown in pages.
- `render_at(comp, t, layout) -> Frame` (a `DisplayList`) and `render_into(.., &mut impl Painter)`. `Painter` has rect, asset, text run and clip calls; `DisplayList` records them and `SoftFrame` rasterizes rects (text runs as solid blocks) for tests.
- `TextLayout` measures and wraps text; `FixedAdvance` is the deterministic stand-in until Parley (slice 2).
- `frame_key(comp, t, layout)`: SHA-256 over frame size, layout id, `t` and the layers active at `t`.

`WordTiming` is defined here because `crastu-captions` has no word type yet. Times are seconds on one timeline. Slice 2 adds Vello and Parley on `wgpu` (`docs/spikes/render-gpu-stack.md`).
