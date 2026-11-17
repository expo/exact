surfaces: web, linux
agent: Codex
task: replace application chrome colours with platform roles

## Rough

- The graphical entry forced light mode after mounting. Its seven palettes also
  fixed the page, panels and text to light colours. Removing that override and
  naming roles lets the host's appearance reach both the chrome and controls.
- Apple's `AccentColorText` stays white on custom accents. Each wallpaper now
  pairs a dark light-mode accent with a pale dark-mode accent and explicit
  white/black ink. The destructive fill has its own red pair. All pairs clear
  4.5:1 against their ink in the Apple and neutral colour columns.
- Completed todos used half-opacity text, and blue event bars had insufficient
  text contrast. Completed labels now use the secondary label role with their
  strikethrough; fixed event fills use black ink in either appearance. Carried
  and landing titles keep opaque ink too.
- Completed checkmarks were white on pale schedule hues, at 1.69:1 on Lilac,
  then dimmed to 55% opacity. The month, inspector and both drag-preview
  paths now keep black checks at full opacity on those fixed content hues.
- A `check both` drive reported dark ink on a light background before the next
  frame settled. The regression drive sets the preference and settles before
  checking each appearance. Screenshots confirm the Classic grid changes its
  background with its text.

## Lean in

- Roles cover every wallpaper without another set of surface and text palettes.
  A scheme-aware gradient veils the wallpaper from 8% to 50%; opaque role
  surfaces behind the header, date chips and schedule text protect contrast
  without covering the photograph.

## Needed

- Appearance and colour roles: provided.
- Native calendar drag input on Linux: missing; the Linux colour drive uses the
  shared host, Calendar data and app storage without that platform input module.

## Checkpoints

- `bun host/web/build.mjs ./examples/calendar` follows all imported components.
- `bun test examples/calendar/tests/appearance.test.mjs` checks every wallpaper,
  a selected date, the wallpaper scrim and the editor in light and dark mode.
  Its native-colour test also checks all five filled-label sites for every theme.
- The browser regression checks completed Lilac marks in the month and
  inspector for all seven themes in both appearances. The runner checks
  carried and landing marks with reduced motion on and off. Both regressions
  fail with the white, dimmed checks before the repair.
- Calendar's shared runner tests check completed-todo ink while carried and
  landing; the test fails at opacity 0.5 before the repair.
- Browser and headless Linux screenshots show readable month and inspector text.
- The remaining 15 warning sites are schedule fills, their drag previews and
  fixed black ink, plus the stored-colour swatches and their checkmarks.

The named graphical entries' `lower-colour-one-scheme` sites, excluding the
compiler's closing summary:

| Entry | Before | After | Retained content |
| --- | ---: | ---: | --- |
| Calendar | 127 | 15 | Schedule hues, previews and swatches, with fixed black ink |
| Messages | 33 | 2 | Conversation/model avatar fills and contrasting initials |
| LLP | 1 | 0 | None |
| Exact Live | 1 | 0 | None |
| Caltrain | 1 | 1 | Artwork dimmer on TV |
| Video-player | 1 | 1 | Black video letterbox |
| Weatherlight | 1 | 1 | Blue sky fallback artwork |
| Carousel | 2 | 2 | White labels on fixed card hues |
| Sparkline | 1 | 1 | Black initials on data hues |

All nine entries were rebuilt. Fixtures, corpus entries, galleries and terminal
entries were outside this repaint. Caltrain's GPU artwork still needs a graphics
adapter for visual qualification.
