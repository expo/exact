# The `rgba8` and `color2` codecs have no row, and their arms are carried in the generator, the wire, the compiler and every host

**Status:** Closed
**Resolution:** Style codecs have schema rows; unused rgba8, color2 and numeric u8 paths are removed, and the codec description matches the schema.
**Systems:** kernel/build.rs, kernel/build/codec.rs, kernel/tables/schema.json, kernel/src/wire/codec.rs, kernel/src/style.rs, hosts' row readers
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

No row in `kernel/tables/schema.json` uses the codec `rgba8` or `color2`: the 233 rows
hold colours as `color` (4), `current-color` (9), `auto-color` (2) and `paint` (5).
The two codecs are still built:

- `kernel/build/codec.rs:101`, `106` parse them, and `:253`–`:438` give each a Rust
  type, a default, a decoder and an encoder. `kernel/build.rs:1287`, `1306`, `1356`,
  `1362` give them a `set_dynamic` arm and the getters that return `RowValue::Color`
  and `RowValue::Color2`.
- `StyleCodec::Rgba8` and `StyleCodec::Color2` (`kernel/build.rs:619`), which no
  `StyleId::codec()` returns, and `RowValue::Color` and `RowValue::Color2`
  (`kernel/src/style.rs:925`, `932`), which nothing constructs.
- `Reader::color2` and `Writer::color2` (`kernel/src/wire/codec.rs:272`, `632`), which
  nothing calls.
- Arms that match the unreachable variants: `kernel/src/style.rs:981`, `983`,
  `kernel/src/style/theme.rs:384`, `host/web/src/css.rs:618`, `766`, `858`,
  `host/apple/src/style.rs:192`, `457`, `host/web-js/src/style.rs:1276`,
  `runner/src/agent/row.rs:104`, `158`, `contract/lower/src/values.rs:282`,
  `contract/lower/src/vocab.rs:304`, `contract/lower/src/theme/scan.rs:36`,
  `contract/cli/src/forma_uses.rs:427`, `contract/cli/src/gallery/inert.rs:398`, and
  five in `contract/cli/tests/it` (`theme.rs:34`, `forma.rs:43`, `461`,
  `state_parts.rs:40`, `actions_exports.rs:199`).

The plain `u8` codec is in the same state: both `u8` rows are `keywords` rows, so
`Codec::U8`'s integer arm (`kernel/build.rs:1283`) is never emitted.

The schema's `_styles` prose (`schema.json:2665`) lists `rgba8` and
`color2 (rgba8 rgba8)` among the codecs and does not list `color`, `current-color`,
`line-height` or the CSS-text codecs added since (`box-shadow`, `text-shadow`,
`filter`, `clip-path`, `corner-shape` and others). It also says a calculation value's
unit byte is 0 to 12; `rem` is 128 (`kernel/src/style/calc/wire.rs:41`).

## The failure

Nothing misbehaves. A reader of `RowValue` in a host handles two variants that cannot
occur, and the schema's description of its codecs does not match its rows.

## Verification

`jq -r '.styles[].codec' kernel/tables/schema.json | sed 's/enum:.*/enum/' | sort | uniq -c`
prints no `rgba8` and no `color2`, and two `u8` (both with `keywords`). The generated
`schema.rs` holds no `RowValue::Color(`, `RowValue::Color2(`, `StyleCodec::Rgba8` or
`StyleCodec::Color2` outside the enum declarations.

With the two `StyleCodec` variants, the two `RowValue` variants and every arm listed
above deleted,
`cargo check --all-targets -p exact-kernel -p contract-lower -p exact-runner -p contract -p exact-web -p exact-web-js -p exact-apple -p exact-linux -p exact-terminal`
passed. Each compile error on the way was a match arm naming a deleted variant; none
was a constructor. A search of the Swift, Kotlin, JavaScript and Lean sources finds
neither codec name. The deletion was reverted.

## Direction

Delete the two codecs, `RowValue::Color` and `RowValue::Color2`, with their arms, and
bring `_styles` in line with `parse_codec`.

## Related

LLP 1095 D1 names `color2` among the colour codecs. Nothing in `issues/` or
`QUEUE.md` names either.
