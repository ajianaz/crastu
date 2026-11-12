# Crastu design system, v1

Status: **locked 2026-11-10**. Changes after this point go through a decision entry in `DECISIONS.md`.
Visual source of truth: the design canvas "Crastu design system" (pages Foundations, Components, Screens).
Static copies of the boards and the tokens as data are in [design/](design/README.md) (`tokens.json`, one HTML file and one screenshot per board).

## 1. Principles

1. **A tool, not a showcase.** The user's media and words stand out. Chrome stays quiet and consistent.
2. **One signal colour.** The accent marks the primary action, the playhead and the current selection. `record` red is used for the recording state only.
3. **Time is data.** Timecodes and durations use tabular monospace digits (`hh:mm:ss.ff`).
4. **AI is visible and steerable.** Every AI suggestion shows a score or reason plus accept and reject. Actions from external agents follow the approval policy and land in one activity log.
5. **Feels native on a Mac.** Keyboard first, ⌘ shortcuts, visible focus, calm motion that respects Reduce Motion.

Surface mode: **Operate** (app UI). Precision and consistency outrank expression.

## 2. User settings that change the look

| Setting | Options | Default | Changes |
|---|---|---|---|
| Colour theme | Graphite, Paper, Signal | Graphite | Colour tokens only |
| Font pairing | Inter + JetBrains Mono, Space Grotesk + JetBrains Mono, IBM Plex Sans + IBM Plex Mono | Inter | `font-ui`, `font-mono` |
| Density | Compact, Comfortable | Compact | Hit targets 28 / 36, paddings |
| Reduce motion | on, off | follows macOS | Disables transitions |

Structure, radius and spacing never change with theme.

## 3. Colour tokens

| Token | Use | Graphite | Paper | Signal |
|---|---|---|---|---|
| `bg` | App background | `#111214` | `#F6F5F2` | `#0A0A0B` |
| `surface` | Panels, sidebars, cards | `#18191C` | `#FFFFFF` | `#121214` |
| `raised` | Secondary buttons, active item, popovers | `#202226` | `#EFEDE8` | `#19191C` |
| `border` | Dividers, edges | `#2C2E33` | `#E0DDD6` | `#26262A` |
| `text` | Primary text | `#ECEDEE` | `#1B1A17` | `#F2F2F0` |
| `text-2` | Secondary text, labels | `#A4A7AE` | `#57544D` | `#A8A8A2` |
| `text-3` | Hints, placeholders | `#868A93` | `#6E6A62` | `#8A8A84` |
| `accent` | Primary action, playhead, selection | `#4CC2B0` | `#2F5BD3` | `#C6F432` |
| `accent-hover` | Primary hover | `#63CDBD` | `#2850BC` | `#D3F75E` |
| `accent-press` | Primary pressed | `#3BA897` | `#2246A6` | `#B0DB22` |
| `on-accent` | Text on accent | `#0B1F1C` | `#FFFFFF` | `#0A0A0B` |
| `accent-soft` | Word highlight, selected fill, "Commercial OK" badge | `#1E3B37` | `#DCE5FA` | `#2E3A10` |
| `record` | Recording state, clipping only | `#FF5A4E` | `#D93A2B` | `#FF4D3D` |
| `danger` | Delete, errors, "Non-commercial" badge | `#F0706A` | `#B42318` | `#FF6B5E` |
| `danger-soft` | Danger badge fill | `#3D1C1A` | `#FCE4E1` | `#3D1512` |
| `on-danger` | Text on danger button | `#2A0B09` | `#FFFFFF` | `#1A0505` |
| `warn` | Warnings, "Conditional" licence | `#E6B450` | `#9A6700` | `#FFC94D` |
| `warn-soft` | Warn badge fill | `#3A2E14` | `#FBEFD5` | `#3A2F10` |
| `media-bg` | Thumbnails, video area | `#26282C` | `#E7E4DD` | `#1E1E21` |
| `media-icon` | Placeholder glyph on media | `#5C6068` | `#A29E95` | `#4A4A4E` |
| `wave` | Waveform bars | `#3A3D43` | `#CFCBC2` | `#34343A` |
| `track-video` | Video clips | `#5B7FD6` | `#3E63C8` | `#7FA2FF` |
| `track-audio` | Audio clips | `#4CC2B0` | `#1F8A78` | `#5BE3C9` |
| `track-caption` | Caption clips | `#C89B4C` | `#9A6A1A` | `#FFC94D` |
| `on-track` | Text on track clips | `#0E1013` | `#FFFFFF` | `#0A0A0B` |

Rules: components reference token names only. A lint in CI rejects colour or font literals outside the token module. Text pairs meet WCAG AA 4.5:1; to be re-checked by an automated contrast test before v1 ships.

## 4. Fixed tokens

| Group | Values |
|---|---|
| Type scale | display 24/600, title 16/600, body 13/400, label 12/500 `text-2`, time 13 mono, score 20 mono/500 |
| Spacing | 4, 8, 12, 16, 20, 24, 32 |
| Radius | control 6, card 10, media 10, pill 22 |
| Hit target | 28 compact, 36 comfortable, 44 for the dictation pill |
| Motion | 120 ms and 200 ms, ease-out; `transform` and `opacity` only |
| Icon | 20 px, drawn on a 24x24 grid with a 1.8 px stroke, round caps and joins, `currentColor` |
| Shell | header 44, rail 64, left sidebar 236 (collapsible), right inspector 316 (collapsible), content fills |

Labels are sentence case and never letter-spaced capitals. No emoji as icons; icons are 1.8 px stroke SVG.

## 5. Layout

Column order: **rail, left sidebar, content, right inspector**.
- Rail: Dictate, Voice, Clip, Edit, Memory; Settings pinned at the bottom.
- Left sidebar content is owned by the active module (sources, sections, voices, media bin, settings categories).
- Inspector shows the selected object (entry, take, clip candidate, timeline clip).
- Header: breadcrumb, global search (⌘K, also searches memory), MCP status.

## 6. Components (all with documented states)

| Group | Components | States |
|---|---|---|
| Core | Button (primary, secondary, ghost, danger), icon button, text field, select/dropdown, segmented control, checkbox, switch, radio, slider, range slider, tabs, badge, kbd, tooltip | default, hover, pressed, focus-visible, disabled, loading; fields add error; toggles add selected |
| Media | Transport, timecode, level meter (safe/loud/clipping), waveform (default/range/playhead), clip candidate card (default, hover, selected, accepted, rejected, processing), timeline track and clip (with trim handles), playhead, job row (queued, running, done, failed) | as listed |
| AI and feedback | Dictation pill (ready, listening, transcribing, cleaning up, pasted, error), inline AI suggestion, toast (success, warning, error), MCP approval dialog with diff (+ add, ~ change, − remove), approval policy row, MCP activity log, empty state | as listed |

Licence badges: `Commercial OK` (accent-soft), `Conditional` (warn), `Non-commercial` (danger), `User-installed` (outline), `Provider terms` (outline), `Local`, `Cloud, your key`.

## 7. Screens in v1

1. Onboarding (macOS): Welcome, Permissions (Microphone, Accessibility, Input Monitoring, Screen and System Audio Recording, optional), Models, Shortcut, AI agents (MCP command, default policy, optional AI provider key), Done.
2. Dictation: history by day, filter by app, inspector with cleaned and raw text, audio, engines, dictionary action.
3. Voice studio: built-in Supertonic presets, user-installed engines, script editor, takes, licence card.
4. Auto-clipper: sources, accepted clips, candidate grid with score and reason, source timeline with markers, 9:16 preview, transcript, framing, accept/reject/open in editor.
5. Timeline editor: media bin, 9:16 preview with safe area, toolbar (select, split, ripple, snap, zoom), tracks V2/V1/A1/A2/C1, inspector (transform, reframe, speed, volume) and AI suggestion.
6. Settings: Appearance, AI providers, Models and licences, Voice engines, Agents (MCP), Memory, Shortcuts, Storage, Privacy (the Offline switch), About.

## 8. Copy rules

- UI, demos and docs are English. Sample user content may be in any language.
- Plain verbs on buttons ("Find clips again", "Accept clip"). No exclamation marks.
- Never invent metrics in the product UI. Unknown sizes show "TBD" until measured.
- Every licence-related surface names the licence and links to its text.

## 9. Not designed yet

Dub module, meeting capture, the in-app agent panel, Windows-specific chrome, the empty and error states of every screen beyond the shared component, localisation.
