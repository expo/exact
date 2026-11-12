# A `table` in a terminal entry draws stray blank lines: its UA spacing and cell padding skip the terminal's space rule

**Status:** Closed
**Resolution:** Under the terminal's space rule a table's `border-spacing` (its own or the UA's), its own padding and a cell's UA padding are whole cells on each axis, in the table, its caption's inset and an anonymous table's grid, so the UA's 2 px and 1 px are none.
**Systems:** kernel/src/style/table.rs, kernel/src/style/cells.rs, host/terminal
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

Under the terminal's space rule (`Env::cell_spaces`, LLP 1101 §4) "a padding, margin
or gap that resolves to points is rounded before layout, so px and the painted cells
agree". `StyleProps::lower` applies it to every padding, margin and gap
(`kernel/src/style.rs:1321`–`1354`, `Dimension::spaced` and `cells::gap`).

`table::apply` runs after it (`style.rs:1465`) and writes px again without the rule:

- the table's padding is the authored one plus the spacing, not `spaced`
  (`kernel/src/style/table.rs:162`–`167`);
- its gap is the spacing in px (`table.rs:168`–`171`), HTML's UA 2 px when unsaid
  (`SPACING`, `table.rs:28`);
- a cell's unsaid padding is the UA's 1 px (`CELL_PADDING`, `table.rs:254`).

So rows of text sit 20 px apart where a terminal row is 16, and the host's snap to
cells opens a blank line every few rows.

## The failure

```
component App
  view
    column
      table
        tr
          td
            text "row 0"
          td
            text "b0"
        tr
          td
            text "row 1"
          td
            text "b1"
        tr
          td
            text "row 2"
          td
            text "b2"
      text "after"
```

`exact-terminal table.contract --size 40x16 print` prints (`·` is a space):

```
row·0·················b0
row·1·················b1

row·2·················b2
after
```

Expected: three rows on three lines, then `after`, as a `column` of `row`s draws.

`border-spacing=0` on the table still leaves the cell padding, which opens a blank
line after the fourth row. The blank lines go only when every `td` also says
`padding=0`. No other spacing compiles in a terminal entry, where `border-spacing`
takes cells and the kernel refuses them on that row.

## Verification

Ran the command above on a `host-dev` build of `exact-terminal`: the output shown.
The same table with ten rows (`row 0` to `row 9`) has blank lines after `row 1`,
`row 5` and `row 9`; with `border-spacing=0`, one after `row 3`; with
`border-spacing=0` and `padding=0` on each `td`, none; a `column` of `row`s, none. A temporary kernel test laid out a four-cell
row under `Kernel::set_cell_spaces(true)`: the table's cells are at x 2, 22, 42, 62,
and a flex row with `column-gap=2` and `padding=2` has its items at x 0, 16, 32, 48.

## Direction

In `table::apply`, round the spacing and the UA cell padding with `cells::space`
under `env.cell_spaces` (2 px and 1 px are no cell), and take the table's authored
padding through `spaced` as `lower` does.

## Related

LLP 1101 §4's row "adjacent borders" says `exact:terminal`'s table draws its own
joins; no such module exists in the repository, and the terminal profile
(`contract/cli/src/terminal.rs:59`) does not refuse HTML's `table`. No issue or QUEUE
entry covers it.
