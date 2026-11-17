# A refused mode-boundary color is latched and never reparsed

**Status:** Closed
**Resolution:** When a mode boundary's tuple refuses the inherited colour text, boundary_rows leaves that text unset, clears the row it implied before and drops its theme row, so the next visit parses the text again; the row is indexed only after the parse succeeds.
**Systems:** runner
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`boundary_rows` stores the inherited token text it meant to apply even when `set_plan_style` fails. The next visit sees the same text and skips the row. No theme-index entry is written, so a later `set_tokens` does not parse that text again. The child keeps the ancestor color, or an older implied color whose index row was left in place.

## Where

`runner/src/instance/theme.rs`, `boundary_rows`. The assignment of `implied[k]` follows the match, including the arm where `set_plan_style` returns an error. A successful parse is what inserts the `ThemedRow`.

## How to reproduce

Give a mode boundary an inherited color token whose text does not parse in the child tuple. Update again with the same text, or correct the token and set it through a path that only reparses indexed rows. The child color stays the one from before the refusal.

## Direction

On a failed parse, leave that text unset. Drop any previous theme row for the style and clear it, as a trapped binding does. Insert the row only after `set_plan_style` succeeds, so a later token update can set it again.
