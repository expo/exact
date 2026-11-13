# A quote in the root route drops the implicit not-found screen

**Status:** Open
**Systems:** contract
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`screen` builds the implicit not-found view as Contract text and interpolates the root route pattern into `link href="{home}"`. A `"` in that pattern ends the attribute. `parse` failure is discarded (`ok()?`), so the screen is omitted. The type checker and the lowerer still add the not-found route when `implicit_notfound` is true, and an unknown URL then has a route and no view. A pattern that remains valid Contract can also add attributes to the link.

## Where

`contract/syntax/src/notfound.rs`, `screen`. `root_pattern` is the first tab row, otherwise the first route. `contract/types` and `contract/lower` add the route from `implicit_notfound` without reading this view.

## How to reproduce

Give the first route a pattern that contains `"`. Open an unknown URL. The not-found screen is absent.

## Direction

Escape the pattern for a Contract attribute, or build the link node without a second parse. Keep the screen when the pattern is not a URL.
