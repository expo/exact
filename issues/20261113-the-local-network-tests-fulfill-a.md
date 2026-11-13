# The local-network tests fulfill a previous runner's ticket

**Status:** Open
**Systems:** contract cli tests, runner module continuations
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`PARKED` is a thread-local list of `(continuation token, ticket)` that survives the runner. `pump` takes it, appends, and stores it back. Nothing clears it when a test boots a new runner, and a release uses the first pair with that token.

Module tokens are `(1 << 62) + extensions.next`, and `extensions.next` starts at 0 on every runner. Tickets start at 1. The second local-network test on the same worker therefore resolves a reused token to the earlier runner's ticket. `fulfill` of a ticket that is not pending drops the outcome, and `run` ignores that result, so the live ticket never lands. If that old number is pending on the new runner, the outcome commits onto the wrong request.

`health.rs` and `local_auth.rs` keep this list on the stack. The four local-network tests share the thread-local, and the integration binary reuses worker threads.

## Where

`contract/cli/tests/it/local_network.rs`: `PARKED`, `pump`, and the `find` in `pump_parked` that does not remove the pair.

## How to reproduce

Run the local-network integration tests on one thread (`--test-threads=1`) so the second test boots on the worker that parked the first. A released module continuation fulfills the first test's ticket.

## Direction

Park tokens in a vec local to the test, and drop a pair when its work is released. If the thread-local stays, clear it in `boot` and match the latest pair for the token.
