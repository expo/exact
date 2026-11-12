# Two guides build a query as `?q=${v}`, which `searchParam` reads back wrong for `&`, `+` or `#`

**Status:** Closed
**Resolution:** `start-here.md` and `recipes/website.md` build the query with `encodeURIComponent(v)`, and the recipe shows `A&B c` read back whole where the unencoded value reads back `A`.
**Systems:** docs/start-here.md, docs/recipes/website.md
**Severity:** P3
**Author:** Claude (Opus 5.5), from GPT-6 Astra's whole-project review (2026-11-10)
**Date:** 2026-11-10

Line references are at `46f3d608`.

## What is wrong

`docs/start-here.md:154-155` and `docs/recipes/website.md:61-62` teach a query as
`` path("home") + `?q=${v}` ``, with `v` written as it is. `searchParam` decodes the query
as `URLSearchParams.get` does (`route/src/location.rs:224-241`): `&` ends the value, `+`
is a space, `%` starts an escape, and `#` ends the query. So a search for `A&B` reads back
`A`, `C++` reads back `C  `, and `#1` reads back an empty string.

`docs/start-here-site.md:67` writes the same thing correctly,
`` `?q=${encodeURIComponent(v)}` ``, and `encodeURIComponent` is in the standard library
(`docs/contract-grammar.md:1056`).

## To see it

Confirmed by reading the code only. Follow `website.md`'s deep-link example with
`v = "A&B"`: the location is `/home?q=A&B`, and `searchParam(top(nav), "q")` is `"A"`.

## What a fix is

Write `` `?q=${encodeURIComponent(v)}` `` in both places, as `start-here-site.md` does,
and add to `website.md` one round trip with a value holding `&` and a space.
