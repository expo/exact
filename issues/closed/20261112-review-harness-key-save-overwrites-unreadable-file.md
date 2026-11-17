# Saving a harness key discards the other keys when the file cannot be read

**Status:** Closed
**Resolution:** Saving a key treats only a missing keys file as empty; any other read error is returned before the file is opened for writing, so its bytes are kept and the key stays for this session only.
**Systems:** harness provider configuration, credential persistence
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `apps/harness/data/src/keys.rs:123` treats every failure
reading the existing `keys` file as an empty file. `write_line()` then
opens that same path with `truncate(true)` at lines 146–151 and writes
only the newly supplied key. A writeable file that cannot be read, or
one containing an invalid UTF-8 byte, loses all previously stored
provider keys and unrelated lines. The operation returns success.

This path backs the normal `setKey` action through `Keys::set` at lines
103–113 and `commands.rs:278`. The UI consequently says the new key was
saved. Other providers lose their saved credentials on restart. Removing
one key with an empty value can truncate the unreadable file to zero bytes.

Reproduced by compiling a small standard-library probe that includes the
actual `keys.rs` module by path. Each controlled file held an existing
`ANTHROPIC_API_KEY` fixture entry. The first also had an invalid UTF-8
byte in a comment; the second was valid text with mode `0200`, allowing
write but not read. Calling `write_line(dir, "OPENAI_API_KEY", "new-fixture")`
produced:

```text
invalid-utf8: read_error=InvalidData, set_succeeded=true, previous_entry_survives=false
unreadable: read_error=PermissionDenied, set_succeeded=true, previous_entry_survives=false
```

The reproduction source is
`/tmp/exact-review-20261112/harness-keys-read-error.rs`; build it with
`rustc --edition=2021` and run the resulting binary. It creates isolated
temporary files and uses placeholder values only. No real key store is read.

Treat only `NotFound` as an empty file. For every other read error,
return an error before truncating or changing the original; the existing
UI already supports keeping a key for this session when saving fails.
Validate that both read failures preserve the exact original bytes,
ordinary updates preserve other entries, and a missing file still works.

Checked the open and closed filesystem issues and all bodies in the
275-issue GitHub snapshot. Existing harness filesystem issues concern
model tools and approval previews. The reopened
`20261110-shelf-and-messages-failed-read-overwrites-stored-file.md`
concerns different apps' JSON stores. No harness key-store report was found.
