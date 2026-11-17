# A parked wasm answer can hold a Web Lock behind storage

**Status:** Closed
**Resolution:** Browser answers now drain their issued storage before requesting a secret lease without changing native or failed-lease behavior.
**Systems:** host/web/module-glue.js, host/web/module-worker.js, js/src/prelude.js
**Severity:** P2
**Author:** Claude (Fable 5.2), from the review of the OAuth sign-out fix
**Date:** 2026-11-14
**Related:** issues/closed/20261114-oauth-sign-out-can-log-out.md

Answer A could hold a secret lease and park behind answer B's storage while B waited for the same Web Lock. A task-delayed storage and lock harness reproduced the cycle in both the page and worker placements: neither answer settled. The existing microtask mocks had hidden it.

The related claim that storage parking dropped a completed lease's completion marker did not reproduce on `aa1570bfd`. Its marker survived in both placements. An initial fix published a completed lease before parking and refused a running body that parked, but blind review showed that a refused JavaScript body keeps running outside its turn and that early publication can replay stale credential writes. That mechanism was withdrawn.

The defect was the order before acquisition. The prelude reported an unheld secret before checking storage the same answer had already issued, so the storage queue's head owner could stop delivering while it waited for the holder's lock. An answer now drains its own operations through their FIFO checkpoints before asking for the lease. It parks without a lock while those operations wait behind another owner. Storage created after the lease request and before its grant is refused before it queues. The page and worker hosts otherwise remain byte-for-byte as they were at `aa1570bfd`: a running body is not refused, nothing is published at a storage park, and ordinary publication and interrupted recovery remain the boundary.

The ordering and request-to-grant rules are gated on the browser-secret host. Native main and worker placements still run an already-requested lease before delivering storage, settle, and admit a following caller. Main placement also admits another lease while the first answer's earlier read remains outstanding. Worker placement on `aa1570bfd` retains that lease until its owner turn finishes, so the stronger cross-placement assertion requested during review is not a regression guard for this change. Whether native worker placement should release sooner is a separate question; this repair does not change it.

The later-task regression suite covers the original cycle, a three-owner FIFO with rejection callbacks, forgetting during the drain, the request-to-grant refusal, a completed body behind a waiter, cleanup ownership, stale-write replay, worker acceptance and forgetting, and POST interruption recovery. It also guards that a rejected or refused lease never invents a credential deletion, including while another answer owns the ambient storage turn. The three ordering cases remain `STILL WAITING` on the base behavior and finish in both placements with this rule. Failed bodies after a partial write characterize the base recovery behavior rather than changing it here. The GET and timer interruption controls and the caught partial-write case are recorded in `issues/20261115-an-interrupted-browser-secret-lease-after-a-safe-wait-keeps-its-tentative-credential.md`.

The wasm OAuth fixture cannot express two answers contending for one secret and storage, so its Chrome drive covers the ordinary sign-in, sign-out and revoke paths instead.

Verification on this branch: the two browser storage files pass 98 tests; the OAuth package passes 115; `exact-js` passes 431 with three intentional ignores; the filtered `exact-data` secret suite passes 13; and the wasm OAuth Chrome drive passes its three web cases with its native-only case skipped. Scoped Rust formatting, `exact-js` clippy, caps, and the all-target workspace check pass. The repository gate passes every step except its known `cargo fmt --all` failure in a worktree nested under the main checkout, where `vendor/ureq` resolves the parent's workspace.

Fixed on this branch.
