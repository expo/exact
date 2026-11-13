# A held app-decides bundle with a rotten plan is never restaged

**Status:** Open
**Systems:** update
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

With `deploy.activate` app-decides, a staged bundle stays pending. `admit_check` treats a pending digest equal to the head as current before it looks at the payload. The current path rewrites the witness and the sequence floor. It does not rewrite the plan. `read_view` accepts the plan when the file is regular and does not hash it. `prepare_activation` then fails in `read_card` when the bytes no longer match the signed card, and pending is left in place.

Every later check of that head is current, so the plan is not downloaded again. That sequence cannot be repaired until the publisher ships a different digest. The same damage on a next-launch selection is cleared and restaged.

## Where

`update/src/store.rs`, `admit_check`, `finish_check`, `read_view`, and `prepare_activation`. `update/src/store/disk.rs`, `read_card`.

## How to reproduce

Stage a bundle under app-decides. Replace `app.plan` with a regular file of the same length and a different hash. Check the same head again, then prepare activation. The check reports current. Preparation fails, and the pending entry is still that digest.

## Direction

If the pending entry fails `read_card`, do not return current. Fall through to `write_entry`. Clear pending when `prepare_activation` cannot read the plan.
