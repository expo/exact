# Two browser answers can deadlock while acquiring secret leases in opposite name order

**Status:** Open
**Systems:** host/web/module-glue.js, js/src/prelude.js
**Severity:** P3
**Author:** Codex, from the parked secret lease ordering reviews
**Date:** 2026-11-15
**Related:** issues/closed/20261114-a-parked-wasm-answer-can-hold.md

The wasm browser host can retain one name's Web Lock while the same answer asks
for another name. This is `aa1570bfd` behavior and was not changed by the
storage-before-acquisition repair in the related issue.

Answer A enters `store.exclusive("foo", ...)`, starts an unawaited
`store.exclusive("bar", ...)` inside the body, and completes the `foo` body.
Answer B does the same in the opposite order: it enters `bar`, starts an
unawaited `foo` lease, and completes the `bar` body. When each answer settles,
the next unheld name is at the front of its secret queue. A still holds `foo`
while asking for `bar`, and B still holds `bar` while asking for `foo`; neither
answer reaches ordinary result delivery, so neither lock is released.

`js/src/prelude.js:588-603` reports the next unheld name after the completed
body. `host/web/module-glue.js:620` retains the answer's turn, and
`host/web/module-glue.js:637-648` asks for the next Web Lock before the turn is
ended. The worker placement has the same turn structure.

A fix must choose the lease model rather than infer one from this cycle: impose
one acquisition order on names, release a completed lease's lock before asking
for another name, or refuse a nested lease by name. A regression should use
barriers around the two first grants and two second requests; it should not
assert a hang as its success condition.
