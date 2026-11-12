# `difftest corpus --js-only` fails two cases on its own driver: a command linked from commands.js runs instead of being recorded, and a select's `value` reads empty

**Status:** Closed
**Resolution:** Fixed before this closing by 38497349 (the driver's fallback proxy ignores the boot's assignment of a linked command) and bfa769b4 (the render DOM reads a select's `value` from its selected option); `corpus --js-only` now agrees on both cases.
**Systems:** semantics/difftest/js/drive.mjs, host/web-js/dom.js, scripts/async.mjs (the `difftest-js` check)
**Severity:** P2
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at `dd6f5dcd`.

1. **A linked command is not recorded.** The driver records host commands by replacing every entry of the runtime's `Hosts` table before the app boots (`semantics/difftest/js/drive.mjs:196`), with a proxy behind the table for names it lacks (`:217`). The commands of `host/web-js/commands.js` are linked by use (LLP 1047.002): the compiled module installs the ones its actions name when it boots (`host/web-js/rt.js:270`, `useCommands`), which is after the driver's replacement (`drive.mjs:226`). So `openURL` runs for real, calls the browser's `open` (`host/web-js/commands.js:38`), which the VM context lacks, and the case is a driver error.
2. **A select's `value` reads `""`.** The driver delivers a typed option with `e.value = text; fire(e, event)` (`drive.mjs:301`). The render DOM's setter marks the matching option `selected` (`host/web-js/dom.js:103`), but its getter returns the element's own `value` attribute (`dom.js:100`), which a select has not. The handler reads `""`, the state becomes `""`, and the next resource call has no answer in the runner's transcript.

## The failure

```
$ cargo run -q -p contract-difftest -- corpus --js-only
ERROR-JS corpus/actions/commands.contract: commands in order: ReferenceError: open is not defined
    at <anonymous> (app.js:4243:11) …
DIVERGE-JS corpus/resources/select-outside.contract: a missing reply after an unsupported select is outside at line 8:
  rust: outcome ok
  js:   outcome refused
  js # js: the oracle has no answer for load["",n0000000000000000]
difftest js: 587 cases: 572 agree, 3 diverge, 11 outside the JS target, … 1 driver errors   (exit 1)
```

Expected: both cases agree with the runner.

## Verification

The command was run and printed the above. Each cause was then confirmed with a temporary edit, reverted afterwards:

- Defining `open` in the context is not enough: `commands.contract` then diverges, `rust: command openURL "https://example.com/?q=-1%20%26%20more"` against `js: view "n" "3"`, because the command line is missing.
- With a `set: () => true` trap on the proxy at `drive.mjs:217`, so that the boot's assignment leaves the recorder in place, `commands.contract` agrees.
- With the render DOM's `value` getter answering a select's selected option's `value`, `select-outside.contract` agrees.
- With both, the whole run is `574 agree, 2 diverge, 11 outside the JS target, 0 driver errors`. The two left are `routes/binding-trap.contract`, the JS target's own defect.

## Direction

Keep the driver's recorder when the module links a command (ignore the assignment, or install the recorders after boot), and answer a select's `value` from its selected option, in `dom.js`'s getter or in the element reads the driver adds (`drive.mjs:70`).

## Related

`issues/closed/20261006-difftest-sound-command-observation.md` was the same loss when sound commands moved into the runtime; `issues/closed/20261006-difftest-input-dom-constructors.md` was an earlier gap of the driver's DOM. Nothing open in `issues/` or `QUEUE.md` names either failure.
