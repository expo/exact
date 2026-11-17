# The design checker accepts a family stack the build refuses, and the build's refusal names no token or line

**Status:** Closed
**Resolution:** Family and type tokens share the compiler's stack validation and report refusals at their design values; stack lowering retains token source spans.
**Systems:** forma/src/typeface.rs, contract/lower/src/type_tokens.rs, contract/lower/src/fonts.rs, contract/cli/src/forma.rs, contract/cli/src/forma_import_css.rs
**Severity:** P2
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

Two functions decide whether a font-family stack is valid, and they disagree (at `dd6f5dcd`).

- `exact_forma`'s `family` (`forma/src/typeface.rs:247-292`) takes any quoted member that holds no quote, and any unquoted run of identifier words.
- The compiler's `family_names` (`contract/lower/src/fonts.rs:498-599`) also refuses an empty name, a name holding `;`, `{`, `}` or `\`, the generics `cursive`, `fantasy`, `math`, `emoji` and `fangsong`, and a CSS-wide word as a member.

The compiler declares every family and type token's stack, read by a row or not, with `Span::default()` (`contract/lower/src/type_tokens.rs:69`). Its refusal is therefore at line 0, column 0 of `app.contract`. It names neither the token nor the design file, so the tools that put a build's refusal on a design line cannot place it.

## The failure

`app.forma`, beside an `app.contract` that reads only `ink`:

```
forma t

theme
  ink = light-dark(#111111, #eeeeee)
  family.text = "Comic Sans MS", cursive
```

- `contract build app.contract` prints ``app.contract:0:0 [lower-font-family-generic] CSS generic family `cursive` has no native mapping in exact2; …``.
- `contract forma check` exits 2 with "the design's build stopped first: [lower-font-family-generic] …" and no file or line (`contract/cli/src/forma.rs:363-367`).
- A design server `transaction` that sets `family.text` to this stack answers `ok` and writes the file, and the app no longer builds.
- `contract forma import css` on a stylesheet whose `:root` holds `--hand: "Comic Sans MS", cursive`, or `--empty: ''`, exits 2 with "the design's build stopped first: … nothing written". Its module doc says a value the compiler refuses is refused by name and the rest written (`forma_import_css.rs:15-19`).

Expected: `exact_forma::check` refuses the stack at the token's line, and the compiler's refusal of a token's stack carries the token's place in the design.

## Verification

Ran the built `contract` on these files: `build`, `forma check`, `forma serve --stdio` and `forma import css`. The outputs are as quoted. The build reported the same line 0, column 0 for `"", system-ui`, `"a;b", system-ui`, `Inter, fantasy` and `"Inter", inherit`, and for a type token, `type.body = 16px/1.4 "Comic Sans MS", cursive`.

## Direction

Make one function the rule: have `typeface::family` refuse what `family_names` refuses, or call one shared check from a crate both depend on. Give `theme_stacks` the token's line in the design as its span.

## Related

QUEUE's "The server's write check: what it still leaves to the build" lists "a refusal outside a style row". That covers the server's accepted write, not the checker or the refusal's missing place. QUEUE's "CSS font generic mappings for cursive/fantasy/…" asks for the mappings.
