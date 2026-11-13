# A split row compares the raw href to the canonical visit

**Status:** Closed
**Resolution:** A split's row link is kept as exact_route::canonical makes it, the form a visit stores, so a row whose href has a fragment, an unencoded query or a dot segment is still its route's detail row.
**Systems:** runner, route
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A split row is the detail of a route when the row's `href` equals that route's visit URL. The row is collected when `route_matches` accepts the href. Matching canonicalizes first: it drops the fragment and encodes the query. The visit stores that canonical URL. The comparison uses the raw href.

A list row whose href is a declared route but not already canonical (a fragment, a space in the query, or a `.` or `..` segment) pushes, then is not that route's selected detail. The route is treated as pushed from inside the detail. The host shows Back and does not select the row.

## Where

`runner/src/runner/split.rs`, the row filter and the link collection. `route/src/table.rs`, `matches_pattern`. `runner/src/runner/router.rs`, `route_matches`.

## How to reproduce

In a navigation split, press a row whose `href` is `/items/1#panel` or `/search?q=a b`, and whose pattern is declared. The detail opens. The row is not selected, and the chrome shows Back.

## Direction

Associate a row when `exact_route::canonical(href)` equals the visit URL. Leave the fragment out of the comparison.
