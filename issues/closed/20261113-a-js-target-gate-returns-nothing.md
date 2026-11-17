# A JS target gate returns nothing, so a gated task or sequence stays idle

**Status:** Closed
**Resolution:** Not a defect: `()=>{gate}` is a Rust `format!` string, so `{gate}` interpolates the expression and the emitted gate is an expression arrow that returns (Bench's web build emits `gated(100,a_10,1,0,()=>s_6(),…)`).
**Systems:** host/web-js
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13
**Related:** issues/closed/20261111-js-target-gated-resource-never-answered.md

A gated task and a sequence compile their gate to a block arrow, `()=>{gate}`. A block arrow returns undefined. `schedule.js` and `transport.js` arm only when `gate() === true`, so the gate stays closed for the life of the page. The key thunks beside them are `()=>expr`, which does return.

A resource `when` is the same wrap, `gate:()=>{g}`. `rt.js` treats any result other than true as closed, so an open gate stays asleep and the resource is never asked. Native hosts arm these when the condition holds.

The closed report `issues/closed/20261111-js-target-gated-resource-never-answered.md` decided a difftest harness was answering a resource the runner had left unasked at boot. The emitter still wraps the gate.

## Where

`host/web-js/src/timers.rs`, the `gated` call and the `$sequence` entry. `host/web-js/src/each.rs`, `arguments`. `host/web-js/src/code.rs`, `expression`, which returns a value. `host/web-js/schedule.js`, `gated` and the gate step. `host/web-js/transport.js`, `look`. `host/web-js/rt.js`, the resource memo that returns asleep when the gate is not true.

## How to reproduce

On the JS target, a `task tick when ready` whose `ready` is true never runs. A `sequence groove when on` with `on` true never plays. A `resource x = src() when ready` stays at its zero after `ready` becomes true. The same plan on the native runner arms all three.

## Direction

Emit `()=>` plus the expression, as the key thunks already do.
