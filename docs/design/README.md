# Design reference

Static copies of the design system v1 boards (locked 2026-11-10), kept in the repository so the GPUI work can be built against them without access to the design canvas.

| File | What |
|---|---|
| [`../DESIGN.md`](../DESIGN.md) | The written spec. It wins when a board and the spec disagree. |
| [`tokens.json`](tokens.json) | Every token from `DESIGN.md` as data: 25 colour tokens for Graphite, Paper and Signal, font pairings, type scale, spacing, radius, hit targets, density, motion and shell sizes. |
| [`index.html`](index.html) | A gallery of the boards; open boards in any theme and font. |
| [`boards/`](boards) | One HTML file per board. Components and Onboarding and Settings are interactive. |
| [`screenshots/`](screenshots) | Each board in Graphite with Inter, at 1x, for reading on GitHub. |

## Boards

| Board | Screenshot |
|---|---|
| Principles and decisions | [Main.png](screenshots/Main.png) |
| Tokens | [Tokens.png](screenshots/Tokens.png) |
| Core components: buttons, fields, choices, sliders, badges | [Components-Core.png](screenshots/Components-Core.png) |
| Media components: transport, meter, waveform, clip cards, tracks, jobs | [Components-Media.png](screenshots/Components-Media.png) |
| AI and feedback: dictation pill, MCP approval, toasts | [Components-AI.png](screenshots/Components-AI.png) |
| 1. Onboarding (macOS) | [Screen-1-Onboarding.png](screenshots/Screen-1-Onboarding.png) |
| 2. Dictation: history | [Screen-2-Dictation.png](screenshots/Screen-2-Dictation.png) |
| 3. Voice studio | [Screen-3-Voice.png](screenshots/Screen-3-Voice.png) |
| 4. Auto-clipper | [Screen-4-Clipper.png](screenshots/Screen-4-Clipper.png) |
| 5. Timeline editor | [Screen-5-Editor.png](screenshots/Screen-5-Editor.png) |
| 6. Settings | [Screen-6-Settings.png](screenshots/Screen-6-Settings.png) |

## Opening the boards

Open `index.html` in a browser, or serve the folder:

```sh
python3 -m http.server -d docs/design 8000
```

A board takes `?theme=graphite|paper|signal` and `?font=inter|grotesk|plex`, for example `boards/Screen-4-Clipper.html?theme=paper`. Fonts load from Google Fonts (all under the SIL Open Font Licence); without a network the boards fall back to system fonts.

## Using them in code

- `crastu-ui` (#6) defines its token module from `tokens.json`, name for name (`text-2`, `accent-soft`, `track-caption`, …). Components reference token names only, as `DESIGN.md` requires; never copy a hex value from a board into a component.
- The boards are mockups: sizes, spacing and states are the reference, the sample content (file names, transcripts, scores) is not.
- Screens show `crastu-app` state. When a board shows data the view models do not have yet, add it to `crastu-app` first, then render it.

## How the boards work

Each board is an `<x-dc>` template plus a small script that returns the values to show (`renderVals()`), with `{{holes}}`, `<sc-for>` loops and `<sc-if>` branches. [`boards/dc-lite.js`](boards/dc-lite.js) is a minimal renderer for exactly that subset, written for this repository (Apache-2.0), so the boards open with no build step and no third-party runtime.

## Changing the design

The design system is locked: a change starts with a decision entry in [`DECISIONS.md`](../DECISIONS.md) and an update to `DESIGN.md`. Then:

1. Edit the board on the design canvas and export its `.dc.html` source.
2. Save it as `boards/<Name>.html`, with its `support.js` script line replaced by `<script src="./dc-lite.js"></script>`.
3. Update `tokens.json` if a token changed, and retake the board's screenshot at 1x in Graphite with Inter.
