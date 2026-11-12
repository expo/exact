# A terminal textarea's ArrowUp and ArrowDown wrap its value at the border box's width, the painter at the content width

**Status:** Closed
**Resolution:** The painter and ArrowUp/ArrowDown take a textarea's width from one helper, `paint::content_cells`, and break its lines by one rule, `paint::wraps_before`, over the painter's clusters.
**Systems:** host/terminal (keys.rs, paint.rs)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Two functions break a textarea's value into visual lines, and at `dd6f5dcd` they take different widths.

- The painter, `field`, breaks at the content box: `let width = content.w.max(1)` (`host/terminal/src/paint.rs:806`, `:828`), where `content` is the frame less its borders and padding (`paint.rs:224-236`).
- ArrowUp and ArrowDown call `visual_lines` with the node's whole box: `self.cells_of(f).map_or(1, |r| r.w.max(1))` (`host/terminal/src/keys.rs:125-127`), and `cells_of` is the frame (`host/terminal/src/host.rs:1291-1301`). The comment on `visual_lines` says its lines are broken "as the painter lays them out" (`keys.rs:267-268`).

With a border or padding on the textarea the two differ by that many columns, so the arrows move the caret through lines the screen does not show.

## The failure

```contract
component App
  state draft = "abcdefghijklmnopqrstuvwxy"
  action write(v: string)
    draft = v
  view
    column
      textarea value=draft input=write appearance="none" autofocus=true width="10ch" rows=4 border="thin solid" padding="0 1ch"
```

The content is 10 columns and the box 14. The screen shows three lines, the caret after `y` at cell (7, 3):

```
┌────────────┐
│ abcdefghij │
│ klmnopqrst │
│ uvwxy      │
```

ArrowUp moves the caret to (3, 2), between `k` and `l`. Expected: (7, 2), between `o` and `p`, the same column one line up (LLP 1101.001 P15: "ArrowUp and ArrowDown move the caret by visual line, keeping the column"). `visual_lines` wrapped at 14 columns, so it saw two lines, `abcdefghijklmn` and `opqrstuvwxy`.

With the value `abcdefghijkl` the screen shows two lines and ArrowUp does not move the caret at all: at 14 columns the value is one line.

A textarea with no border and no padding (`textarea value=draft input=write autofocus=true width="10ch" rows=3`) moves correctly.

## Verification

A temporary test booted each source full screen (`Host::boot`, `Mode::Fullscreen`), pressed the key with `host.key(Key::Named("ArrowUp"))` and printed `host.frame().grid.cursor`:

```
25 letters, border and padding: caret at end Some((7, 3)), after ArrowUp Some((3, 2))
12 letters, border and padding: caret at end Some((4, 2)), after ArrowUp Some((4, 2))
12 letters, no border, no padding: caret at end Some((2, 1)), after ArrowUp Some((2, 0))
```

## Direction

Give `visual_lines` the width `field` uses, the content rectangle's columns, from one helper both call. The two break tests also differ by `col > 0` (`keys.rs:281` has it, `paint.rs:828` does not); make them one.

## Related

The coding harness's prompt has its border and padding on the row around the textarea (`apps/harness/terminal.contract:293-295`), so it does not show this.
