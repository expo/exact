# A package view accepts an enum whose cases are not the ones it declares

**Status:** Closed
**Resolution:** A typed prop takes an enum only when the declaration names every case it can take, and an event handler only when its parameter holds every case the event carries, under `option` and `list` too.
**Systems:** contract lower, package views
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`fits` in `contract/lower/src/native_typed.rs` treats every `Ty::Enum` as matching any declaration that starts with `enum<`. It never compares cases. `names_case` does, and only for a string literal.

A prop declared `enum<rent|order>` therefore accepts a state, a `match`, or any other expression typed `enum<plain|buy>`. The same predicate is the event-handler check (`view_event`): an action parameter typed `enum<plain|buy>` is accepted for an event that carries `enum<rent|order>`. `option<enum<…>>` and `list<enum<…>>` recurse into the same arm.

The type checker already compares cases. `can_unify` in `contract/types/src/checks.rs` admits `enum<B>` as `enum<A>` only when every case of `B` is a case of `A`. A string literal on the view is refused by `names_case`. The typed path is the one that stays open, so the value the package said it would not receive is what the host is sent.

## Where

- `contract/lower/src/native_typed.rs` `fits`, the `Ty::Enum` arm
- `contract/lower/src/native.rs`, the `contract-view-prop` check, which calls `fits` and then `names_case` only for `Expr::Str`
- `contract/lower/src/native_typed.rs` `view_event`, which calls `fits` on each event argument

## How to reproduce

Declare a package view with `mode: enum<rent|order>`. In the app:

```
state mode: enum<plain|buy> = "plain"
exact-example mode=mode
```

`contract build` accepts it. The same file with `mode="plain"` is `contract-view-prop`.

## Direction

Parse the declaration's cases and require every case of the value's `Ty::Enum` to be one of them. For an event, the cases run the other way: the action parameter has to be able to hold every case the event carries. Cover both under `option` and `list`.
