# A mixed-case color profile name cannot be referenced

**Status:** Closed
**Resolution:** The shared `color(--name …)` parse keeps an app profile's dashed ident as written, so the kernel and the bake bind the profile it names; standard spaces still match in any case.
**Systems:** color, kernel style, bake
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`color(--Name …)` is parsed by lowercasing the whole function, dashed ident included. A `color-profile` declaration keeps the author's case, and both the kernel and the bake compare the name with `==`.

`color-profile --Brand` then `color(--Brand 1 0 0)` looks up `--brand`. The bake reports that no such profile is declared. If a profile named `--brand` also exists, the colour binds to that one. The predefined spaces still match because their names are already lowercase. A dashed ident is case-sensitive in CSS, and the declaration parser accepts either case.

## Where

- `color/src/lib.rs` `parse_profiled` (`to_ascii_lowercase` on the whole string)
- `contract/syntax/src/parser/color_profile.rs` (the name is stored as written)
- `kernel/src/style/profiled.rs` `parse_profiled` (`declared` compared with `==`)
- `bake/src/colors.rs` `profiled_colors`, which uses the same parse

## How to reproduce

```
color-profile --Brand src="assets/brand.icc" rendering-intent="perceptual"
component App
  view
    box background-color="color(--Brand 1 0 0)" width=20 height=20
```

The build's colour check says no `color-profile --brand` is declared.

## Direction

Lowercase only the `color(` prefix and the predefined space names. Compare a custom dashed ident as CSS does, with its case intact, in the parser, the kernel lookup, and the bake.
