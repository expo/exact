# Feedback redaction misses credentials whose JSON keys are quoted

**Status:** Closed
**Resolution:** Redaction recognizes credential keys that end in a quote (JSON, JS objects and computed keys, JSON escaped inside a string) and blanks a quoted value whole, through its escaped quotes, before diary and attachment upload.
**Systems:** scripts/feedback.mjs, credential redaction
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P1
**Related:** issues/closed/20261102-feedback-redactor-leaks-authorization-headers.md

At `f6be99af`, the generic credential patterns in `scripts/feedback.mjs:70-73`
require a key to be followed directly by optional whitespace and `:` or `=`.
They do not allow the closing quote of a JSON property. Thus ordinary
`password`, `api_key` and `refresh_token` JSON fields survive redaction unless
their values independently match a provider-specific token pattern.

Both diary content and consented README/AGENTS attachments pass through this
redactor before `send`. A note containing a diagnostic JSON object can
therefore upload a credential to the feedback collector. A project set to
`always` sends a note without another preview. The instructions to avoid
writing secrets remain useful, but the credential filter misses a common
format it can recognize.

## Evidence

Executed the actual `redact` and `send` under Bun 1.4.2 using a temporary
fixture and an injected fetch that captures the body without sending it.
These synthetic fields survived both `redact` and the final diary and README
payloads:

```json
{"password":"SyntheticPassword123!","api_key":"SyntheticOpaqueKey123","refresh_token":"SyntheticRefresh123"}
```

The probe is `/tmp/exact-review-20261112/feedback-quoted-probe.mjs`. No real
credentials were read and no network request was made.

## Fix and validation

Recognize quoted credential keys and their complete string values, including
escaped quotes, without exposing a suffix. Check the final request body for
JSON embedded in prose and code fences, single-quoted diagnostic objects,
mixed case, and existing unquoted/header formats.

Checked open and closed filesystem issues and all 275 GitHub issues fetched
for this review. The related closed issue fixed Authorization and
Proxy-Authorization headers, including their JSON forms. Those use a separate
pattern that allows the quote; generic JSON credential fields remain exposed.
