# A trap that poisons the app names only a `pc`, and nothing maps a `pc` to its source

**Status:** Open
**Systems:** host/web-js/budget.js (`Trap`), host/web-js/rt.js, runner (instance errors, poison journal), contract source map (`app.plan.map.json`)
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/closed/20261030-binding-trap-poisons-the-runner.md

## What is wrong

A trap outside a prop or style binding, such as `IterationLimit` in a `derive`, still poisons the runner (LLP 1017.003 D3, LLP 1090 D6; the 2026-10-30 issue made only a binding's trap recoverable). What the app's author then sees is the trap's kind and a program counter. On the web JS target the trap is `new Trap(kind, pc)` with the message `Trap(IterationLimit { pc: N })` (`host/web-js/budget.js:9`, `:100`), and the poison is journaled as `poisoned: Instance(Trap(…))` (`rt.js:219`); later events say `the runner is poisoned; reload`. Natively the journal reads the same (`… refused: Instance(Trap(IterationLimit { pc: 343 }))`, then `… refused: Poisoned`).

`app.plan.map.json` maps nodes, slots, derives, resources and actions to source lines, but not program counters: its keys at `49ae6357` are `digest`, `nodes`, `sources`, `slots`, `derives`, `resources` and `actions`. And a child component's derive is inlined at each read, so it is not in the map's `derives` at all. The bound that was hit (65,536 list steps an evaluation, `MAX_LIST_STEPS`) is not named either. A binding's trap, by contrast, is journaled with its site (`view 2 (site 1): background-color trapped …`).

## The failure

An agent building a multi-screen app (web, exact2 `94b8671f`) had the whole app stop with `clock refused advance: Poisoned`; the console named only a `pc`. Finding the culprit, a list `.includes` inside a `.map` over 225 × 200 items, meant reading the generated JavaScript.

Checked by reading the code and an `app.plan.map.json` built here; the trap was not reproduced.

## Fix

Put the evaluation's site into the trap's journal line: the derive's (or action's, or row initializer's) name, its source line from the map, and the bound in words (`passed the 65,536 list steps one evaluation may take`). The compiler knows each `pc`'s expression when it lowers, so a `pcs` table in `app.plan.map.json` (start and end `pc` per declaration) would let both the runner and the JS target name it, and let `agent` print it.
