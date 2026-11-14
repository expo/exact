# A shorter param name is blamed for a longer one's use

**Status:** Closed
**Resolution:** `reached` matches whole param tokens, as `unused` does, and reports each param whose token occurs.
**Systems:** contract
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`reached` decides that a param hit a non-row place with `text.contains(token)`, and the same `contains` on the component's `Debug` dump. A token is `param.f<8 hex>.<Component>.<param>`, so `param.fXXX.Button.color` is a substring of `param.fXXX.Button.colorActive`. `find` returns the first declared param, so the diagnostic names `color`. The `Debug` scan reports every prefix match. `unused` already splits on identifier characters and matches whole tokens.

## Where

`contract/lower/src/params/rewrite.rs`, `reached`. Tokens are built in `contract/lower/src/params.rs`, `token_name`. The theme calls `reached` with every param.

## How to reproduce

A component that passes both `color` and `colorActive`, and uses only `colorActive` in text, a condition, or a state. `tamber-param-scope` fails against `color`. Editing `color` leaves the failure, because `colorActive`'s token still contains `color`'s.

## Direction

Match whole tokens the way `unused` splits words, and report each param whose token occurs.
