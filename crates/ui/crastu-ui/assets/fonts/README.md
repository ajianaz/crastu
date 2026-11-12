# Bundled fonts

All families are under the SIL Open Font Licence 1.1 (commercial use and redistribution allowed; the licence text sits next to each family). They are the unmodified TrueType files of the official releases.

| Family | Files | Source | SHA-256 |
|---|---|---|---|
| Inter 4.1 | `inter/Inter-Regular.ttf` | https://github.com/rsms/inter/releases/tag/v4.1 (`extras/ttf`) | `40d692fc…e0c82` |
| | `inter/Inter-Medium.ttf` | same | `97ad806f…a872` |
| | `inter/Inter-SemiBold.ttf` | same | `78a843fa…cde3` |
| JetBrains Mono 2.304 | `jetbrains-mono/JetBrainsMono-Regular.ttf` | https://github.com/JetBrains/JetBrainsMono/releases/tag/v2.304 (`fonts/ttf`) | `a0bf60ef…138f` |
| | `jetbrains-mono/JetBrainsMono-Medium.ttf` | same | `31c92d01…4d3d` |
| Space Grotesk 2.0.0 | `space-grotesk/SpaceGrotesk-Regular.ttf` | https://github.com/floriankarsten/space-grotesk/releases/tag/2.0.0 (`ttf/static`) | `5ede28c4…98bd` |
| | `space-grotesk/SpaceGrotesk-Medium.ttf` | same | `3183b7eb…4b44` |
| | `space-grotesk/SpaceGrotesk-Bold.ttf` | same (the family has no SemiBold; the 600 weight of the type scale uses Bold) | `7209bbb7…6bdb` |
| IBM Plex Sans 1.1.0 | `ibm-plex/IBMPlexSans-Regular.ttf` | https://github.com/IBM/plex/releases/tag/%40ibm%2Fplex-sans%401.1.0 (`fonts/complete/ttf`) | `975dcda3…5fa5` |
| | `ibm-plex/IBMPlexSans-Medium.ttf` | same | `331c8639…f907` |
| | `ibm-plex/IBMPlexSans-SemiBold.ttf` | same | `a20caf82…7bb1` |
| IBM Plex Mono 2.5.0 | `ibm-plex/IBMPlexMono-Regular.ttf` | https://github.com/IBM/plex/releases/tag/%40ibm%2Fplex-mono%402.5.0 (`fonts/complete/ttf`) | `7c6fbddc…5a43` |
| | `ibm-plex/IBMPlexMono-Medium.ttf` | same | `98fbd727…c97c` |

The three pairings of `docs/DESIGN.md` are all bundled. The weights are the ones the type scale uses (400, 500 and 600). IBM Plex's licence names "Plex" as a Reserved Font Name, so the files are shipped unmodified. The licence text of `ibm-plex/` is the one that ships with both Plex archives (identical).

`cargo xtask licences` (issue #24) must list these files in the generated NOTICE.
