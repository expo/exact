# clock until can put a password's revealed value in its error

**Status:** Open
**Systems:** scripts/agent-held.mjs, agent diagnostics
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `untilClock` deliberately compares revealed state and then
reads masked state for reporting. Two error paths bypass that separation:

- If the final masked read rejects or no longer contains the requested
  field, line 98 keeps the earlier revealed value. The failure message at
  lines 100–104 prints it.
- A missing nested field is formatted during the revealed read at line 72.
  That diagnostic includes the enclosing value. Since it has no `value`
  property, it never takes the masked-read branch.

A failed drive can therefore put a password into console output or a saved
test log, even though the command never requested revealed output. A typo
such as `password.length` is enough for the second path. The code's stated
contract is that password text is compared revealed and reported masked.

Reproduced by importing the actual `untilClock` on Bun 1.4.2 with a
controlled session. Its revealed state contains only the synthetic value
`synthetic-password-value`; its masked state contains bullets. No real
credentials or app storage were used:

```text
masked-read-fails: password was "synthetic-password-value"
missing-field: password has no field "length" (it is "synthetic-password-value")
```

The first case makes the masked read throw a controlled transport error.
The second uses `until state password.length == "other" within 10` and a
working masked reader. The full executable probe is
`/tmp/exact-review-20261112/until-clock-masking.mjs`.

Keep raw comparison values out of diagnostic objects. Format missing-field
errors from masked data, or describe only the path and type. If masked data
cannot be read, report that the value is unavailable; never fall back to
revealed data. Cover failed masked reads, fields removed between reads and
invalid nested paths through password strings and enclosing records.

Checked open and closed filesystem issues and all 275 cached GitHub issues
for clock waits, masking, revealed state and password diagnostics. No
matching report was found.
