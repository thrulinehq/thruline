# F11 — the screen-reader walk (VoiceOver, NVDA)

F11's bar, verbatim: **"axe 0 serious; VoiceOver and NVDA read every state as its word"**.
The axe half is `scripts/testing/axe-sweep.mjs`. This is the other half: a walk a person
runs with a real screen reader, because no script can hear what one says.

Until both walks below are run and recorded, F11 is reported as
**"axe <result> · VoiceOver could not run · NVDA could not run"** — never as passed.

## Why an agent did not run it

- **VoiceOver** is on this Mac (`/System/Library/CoreServices/VoiceOver.app`, version 10).
  Running it turns on a system accessibility feature that speaks aloud and takes over
  the keyboard on the founder's screen. An agent does not change that setting. **Could not run → founder.**
- **NVDA** runs on Windows only. **Could not run → founder.**

What an agent could run is recorded as a proxy, not a substitute:
`scripts/testing/glyph-ax-probe.mjs` reads Chrome's accessibility tree (the tree VoiceOver
reads from) and checks that every state-carrying node's text holds its word and no glyph.

## Setup (both readers)

1. `cd ui && npm ci && npm run build`, then `thruline ui` with `HOME` and `THRULINE_HOME`
   pointed at a throwaway directory, or use the harness page:
   `cd ui && node ../docs/loop/p6b/a11y-drive.mjs <scratch-dir>` and serve that directory on
   127.0.0.1 (synthetic data only).
2. Open `app.html?altitude=explore#/changes/ch1` (the Change screen) and
   `app.html?altitude=explore#/a11y` (the Trace).

## The walk

VoiceOver: VO = Control+Option. Move with VO+Right; read the current item with VO+A.
Turn on the caption panel (VoiceOver Utility → Visuals → Show caption panel) and copy
what it shows for each step. NVDA: browse mode, Down Arrow; copy from the Speech Viewer
(NVDA menu → Tools → Speech Viewer).

| # | Where | Move to | Must be heard | Must NOT be heard |
|---|---|---|---|---|
| 1 | Change screen | the state chip beside the title | its state word (e.g. "Proven", "Not proven", "Recorded") | "white circle", "black circle", "dotted circle", a colour name |
| 2 | Change screen | the verdict word in the side panel | the word, then the "What would prove it" clause | a glyph name |
| 3 | Trace (Explore) | a card carrying **Proven** | "Proven" | "black circle" |
| 4 | Trace (Explore) | a card carrying **Not proven** | "Not proven" and its clause | "white circle" |
| 5 | Trace (Explore) | a card carrying **Recorded** | "Recorded" | "dotted circle" |
| 6 | Trace (Explore) | a card or row that **needs a re-check** | the words the screen shows for it | any glyph name |
| 7 | Trace (Explore) | a **Fog** line | "Fog" | "midline horizontal ellipsis" |
| 8 | Trace → Outline (sidebar) | the first three rows | each row's state word | any glyph name |
| 9 | Trace (Pro) → Timeline | a row with a claim mark | "claim placed here" | "white circle" |
| 10 | Trace (Pro) → Events | the replay-cursor row | "the replay is at this step" | "black right-pointing triangle" |

Words follow today's vocabulary; when N2-VOCAB (rulings #1–#4) changes a word, re-run
with the new word in the "Must be heard" column.

## Record

For each reader: date, reader and version, browser and version, the caption-panel /
Speech Viewer text per row, and the bucket — **passed** (every row heard as its word),
**failed** (name the row and what was heard), or **could not run** (say why).
