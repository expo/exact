# execFile refuses UTF-8 output within maxBuffer because its JSON reply exceeds a base64-sized allowance

**Status:** Closed
**Resolution:** `execFile` output crosses as base64 in either encoding, decoded as UTF-8 by the binding when asked, and a process reply's capacity is its length, so any output within `maxBuffer` fits the reserved ceiling.
**Systems:** data/src/process.rs, js/src/process.js, host/apple/src/executor_process_supervise.rs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At reviewed commit `f6be99af`, both `execFile` bindings reserve the response
allowance as twice the base64 expansion of `maxBuffer`, plus 64 KiB
(`data/src/process.rs:11-14`, `js/src/process.js:194`). This also applies to
`encoding: "utf8"`. The host serializes UTF-8 output as a JSON string
(`host/apple/src/executor_process_supervise.rs:669-697`), where one control byte
can occupy six bytes, such as NUL becoming `\u0000`.

The host then passes that response through `bounded_outcome`
(`host/apple/src/executor_core.rs:816`), which refuses it if retained capacity
exceeds the allowance (`executor_outcome.rs:8-16`). A successful child whose
output is within `maxBuffer` therefore becomes a failed request, losing its
output and exit result. macOS and Linux share this process transport.

## Verification

A temporary probe used the current Rust `exec_file` request builder and
`bounded_outcome` through their source files, and unchanged copies of the
host's `exec_reply` and `reply` serialization functions. Each pipe contains at
most `maxBuffer=100000` bytes; the child result is exit code zero, no timeout
and no overflow:

| Output | Encoding | Reply bytes | Retained capacity | Allowance | Result |
| --- | --- | ---: | ---: | ---: | --- |
| 100,000 NUL bytes on stdout | UTF-8 | 600,081 | 1,048,576 | 332,208 | Refused |
| 100,000 ASCII bytes on both pipes | UTF-8 | 200,081 | 200,100 | 332,208 | HTTP 200 |
| 100,000 bytes on both pipes | Buffer | 266,753 | 266,772 | 332,208 | HTTP 200 |

The failure is `refused: native outcome exceeds retention limit`.
Running the actual `js/src/process.js` binding over a capture seam independently
confirmed that `execFile(..., {encoding: "utf8", maxBuffer: 100000})` requests
the same `332208` allowance.

Repro sources are `/tmp/exact-review-20261112/process_ceiling.rs` and
`/tmp/exact-review-20261112/process-ceiling.mjs`. Ran the Rust probe linked
against this checkout's compiled libraries and the JS probe with pinned Bun
1.4.2. No child process was launched; this exercises request sizing, response
serialization and the production retention check.

## Direction and validation

Use a wire representation with a predictable bound, or size the allowance
for the selected encoding and its retained allocation. Keep the process
lane's memory bound enforceable. Verify control characters, invalid UTF-8,
both full pipes and ordinary text, including the largest supported
`maxBuffer`; an in-bound output must preserve its successful result.

## Duplicate check

Searched open and closed filesystem issues and all 275 GitHub issues retrieved
on 2026-11-12. Closed `20261110-execfile-keeps-reading-past-maxbuffer.md`
concerns a child continuing to write after overflowing its pipe limit. This
failure occurs without an overflow, during serialization of retained output.
GitHub #206 concerns the separate bake filesystem tool's asset reply buffer.
