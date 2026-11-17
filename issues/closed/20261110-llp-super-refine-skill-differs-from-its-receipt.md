# The installed llp-super-refine skill does not match the hash in the skills receipt

**Status:** Closed
**Resolution:** The receipt was right (it is the `v0.5.2` tag's hash) and the skill was edited here to name current default models, so the receipt now records `llp-super-refine/SKILL.md` as detached and `agents/openai.yaml`, part of no release and read by nothing in `.claude/skills`, is deleted.
**Systems:** .llp/skills-receipt.json, .claude/skills/llp-super-refine
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

`.llp/skills-receipt.json` records the six LLP skills installed from the `v0.5.2` release, with a SHA-256 for each file. `llp-adopt` calls the receipt "the trust root for updates" and says what a mismatch means (`.claude/skills/llp-adopt/SKILL.md:30-32` at `dd6f5dcd`):

> Update MUST three-way-check each file against the receipt hash. Any mismatch is a local fork: abort the coupled update, never clobber, and offer discard (restore managed copy) or detach (file leaves the managed set permanently; receipt records it …).

Five of the six recorded hashes match the files in the tree. The sixth does not, although the file's frontmatter still says `source: ccheever/llp@v0.5.2`:

| File | Receipt | Tree |
|---|---|---|
| `.claude/skills/llp-super-refine/SKILL.md` | `2673e8ca…ef80ba2e` | `a09955eb…788ef6d2` |

The file and the receipt entered this repository in one commit, `0c881ad9`, already differing, and neither has changed since. So the history here does not say which is right: the skill was edited after it was installed and the receipt not told, or the receipt was written from other bytes than the ones committed. The difference is not one of line endings: the file with its final newline removed or added, or with CRLF line ends, does not hash to the receipt's value, and it has no trailing spaces to strip.

A second file sits in the managed directory and is in no receipt: `.claude/skills/llp-super-refine/agents/openai.yaml`.

## The failure

```
$ shasum -a 256 .claude/skills/*/SKILL.md
a4ae6b27…  .claude/skills/llp-adopt/SKILL.md          (receipt: the same)
8711aa16…  .claude/skills/llp-create/SKILL.md         (the same)
e956c3fe…  .claude/skills/llp-maintain/SKILL.md       (the same)
6a421cd0…  .claude/skills/llp-orient/SKILL.md         (the same)
b79df36f…  .claude/skills/llp-review/SKILL.md         (the same)
a09955eb…  .claude/skills/llp-super-refine/SKILL.md   (receipt: 2673e8ca…)
```

Observed: by the rule above, the next skills update stops at this file as a local fork, and the receipt's `note`, "Updated in exact2 from the verified v0.5.2 tagged release", is not true of one of the six files it lists. The receipt has no record that the file was detached. Expected: the receipt describes the files that are installed, so an update goes through and a real local edit is told apart from none.

## Verification

Ran the command above and compared each line with the receipt's `files`. `git log` on the receipt and on `.claude/skills` lists `0c881ad9` alone. Hashed the file under the line-ending variants named above. Not done: comparing the file with the `v0.5.2` release's copy, which would say which of the two is right, and running `/llp-adopt update` (both fetch from the network).

## Direction

Compare the file with the release's. If it is the release's, correct the receipt's hash. If it was edited here, restore the managed copy or record the detach in the receipt, as `llp-adopt` offers. Either way decide whether `agents/openai.yaml` belongs to the installation, and list it or delete it.

## Related

No issue or QUEUE entry mentions the receipt.
