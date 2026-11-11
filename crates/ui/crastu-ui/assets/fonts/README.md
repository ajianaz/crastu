# Bundled fonts

Both families are under the SIL Open Font Licence 1.1 (commercial use and redistribution allowed; the licence text sits next to each family). They are the unmodified TrueType files of the official releases.

| Family | Files | Source | SHA-256 |
|---|---|---|---|
| Inter 4.1 | `inter/Inter-Regular.ttf` | https://github.com/rsms/inter/releases/tag/v4.1 (`extras/ttf`) | `40d692fc…e0c82` |
| | `inter/Inter-Medium.ttf` | same | `97ad806f…a872` |
| | `inter/Inter-SemiBold.ttf` | same | `78a843fa…cde3` |
| JetBrains Mono 2.304 | `jetbrains-mono/JetBrainsMono-Regular.ttf` | https://github.com/JetBrains/JetBrainsMono/releases/tag/v2.304 (`fonts/ttf`) | `a0bf60ef…138f` |
| | `jetbrains-mono/JetBrainsMono-Medium.ttf` | same | `31c92d01…4d3d` |

The weights are the ones the type scale uses (400, 500 and 600). Space Grotesk and IBM Plex, the other two pairings of `docs/DESIGN.md`, are not bundled yet: until they are, choosing them falls back to a system font (`FontFamilies::resolve`).

`cargo xtask licences` (issue #24) must list these files in the generated NOTICE.
