# Web stream and Rust fetch adapters drop the request's authority origin

**Status:** Open
**Systems:** web network admission, host/web/http-body.js, host/web-js/admission.js, host/web-js/ts-stream.js, host/web-js/rust-data.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P1

## Problem

A non-app request reaching the JS target's stream adapter or Rust request
executor is sent with app authority. With local-network permission still
unanswered, these paths reach a private address and follow redirects where
the shared admission code requires `person-required`. A tool, wake or device
event can consequently initiate network access that should await the person.
The network grant is still checked; this bypasses the additional origin and
permission rule, not that grant.

Reviewed at `f6be99af`:

- `host/web/http-body.js:619-622` reconstructs the request for `streamed`
  without `origin` or `reachState`. Its `request` function checks those fields
  at line 472 and uses them for redirect admission.
- `host/web-js/admission.js:181-182` calls `fetchWith` from
  `createRequestExecutor` without its origin argument, whose default is
  `app`. Rust ordinary requests use this executor
  (`host/web-js/rust-data.js:288-290`).
- `host/web-js/ts-stream.js:47` constructs its stream request without the
  originating answer's authority either. Carrying a field in `streamed`
  alone is therefore insufficient: capture the authority before the source's
  turn ends and propagate it through each adapter.

`reachRefusal` and `redirectFor` in `host/web/address-space.js` intentionally
skip their checks when origin is missing or `app`. LLP 1133.036 D6 requires
the check for non-app requests, including each WebSocket open.

## Evidence

Ran the actual `request`, `streamed`, `createRequestExecutor` and
`address-space` modules with a validated exact-origin fetch grant, a public
page (`app.example`), local and loopback permissions set to `prompt`, and a
fetch substitute that records calls. No LAN server was contacted.

The same request carried `origin: 'tool'` and URL
`http://192.168.1.40/json`. Results:

```text
request:               kind=2, NotAllowedError: person-required, 0 fetches
streamed:              kind=0, fetch called with redirect=follow
createRequestExecutor: status=200, fetch called with redirect=follow
```

This establishes the loss at both adapter boundaries. It does not establish
how a particular browser displays its Local Network Access prompt.

## Fix and validation

Capture and carry the answer's origin through the Rust and TypeScript request
paths, including streams, deferred opening and redirects. An absent internal
origin must not silently elevate a non-app operation to app authority.

Add integration cases for requests caused by a tool and a device event from
a public page, through Rust ordinary fetch and TypeScript/Rust streams. With
permission `prompt`, `unknown` or `denied`, the transport must not start;
with `granted`, it may. Check WebSocket opens and public-to-private redirects
as well as direct private fetches. App-origin behavior remains allowed.

## Duplicate check

Checked open and closed filesystem issues and all 275 GitHub issues fetched
on 2026-11-12. The closed
`20261102-tool-handler-fetch-runs-as-the-app.md` fixes lost async context in
the ordinary TypeScript fetch path. The closed
`20261102-fetch-redirect-skips-the-local-network-check.md` fixes the common
policy itself. Here those checks work when called directly; these adapters
discard the inputs that invoke them. No matching GitHub issue was found.
