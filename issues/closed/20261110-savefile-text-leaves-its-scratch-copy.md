# `saveFile(text=…)` leaves a copy of the text in the temporary directory after every save

**Status:** Closed
**Resolution:** `save` hands `presentSave` a cleanup that removes the text's `exact-text-<uuid>` directory, which runs when the macOS panel ends, once iOS has staged its export copy or refused, and at tvOS's refusal; a failed write removes it too.
**Systems:** host/apple/Sources/ExactKit/SaveFile.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

For `saveFile(id, text=…, suggestedName=…)`, `Picker.save` (`host/apple/Sources/ExactKit/SaveFile.swift:34-45`) writes the text to `<tmp>/exact-text-<uuid>/<name>` and hands that file to `presentSave`. Nothing removes the directory afterwards:

- macOS `presentSave` (`SaveFile.swift:143-158`) copies the file to the chosen URL, or cancels, and returns.
- iOS `presentSave` (`SaveFile.swift:113-129`) stages a second copy under `exact-export-<uuid>`, and `exportFinished` (`SaveFile.swift:132-137`) removes only that one.
- tvOS refuses (`SaveFile.swift:104-107`) and removes nothing.

The other scratch copies made here are removed when their sheet closes: a share's `exact-share-<uuid>` (`Share.swift:54`, `discard`) and the export's `exact-export-<uuid>`.

## The failure

```contract
  action exportNotes
    saveFile("export-file", text=notes, suggestedName="notes.txt")
```

Each run of the action outside the agent, saved or cancelled, leaves one more `exact-text-<uuid>/notes.txt` holding the exported text. An app that saves a document this way keeps every version it exported until the system clears its temporary directory. Under the agent the request is held and no scratch file is written.

Expected: the scratch file goes when the panel or picker ends, as the share's and the export's do.

## Verification

By reading. `exact-text-` appears once under `host/apple`, at the directory's creation, and nothing sweeps the temporary directory at launch. Each of the three `presentSave` bodies, `copyOut`, `saved`, `saveCancelled` and `exportFinished` was read; none removes the directory of the file it was handed. Not run: the path needs a save panel answered by a person, since the agent's answer bypasses it.

## Direction

Remove the scratch directory when the save ends: `save` passes a cleanup that `presentSave`'s completion runs (the panel's handler, `exportFinished`, the tvOS refusal), as `share` passes `discard`.

## Related

`issues/closed/20261102-two-photo-saves-share-one-temp-path.md` and `issues/closed/20261102-photo-copies-collide-across-sessions.md` are about the media picker's names, not this file. QUEUE.md has no entry for `saveFile`'s scratch file.
