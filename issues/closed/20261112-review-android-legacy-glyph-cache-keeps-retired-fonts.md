# Android's legacy glyph cache keeps every retired font and outline

**Status:** Closed
**Resolution:** The cache holds each face weakly by identity, so a retired reader's faces and their outlines are collected, and keeps only each face's 2,048 most recently drawn outlines beside the existing 128 images, whose keys no longer keep a face alive.
**Systems:** host/android/canvas/src/main/java/dev/exact/canvas/LegacyGlyphs.kt, host/android/canvas/src/main/java/dev/exact/canvas/ExactView.kt
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, Android 10 and 11 draw glyphs through the process-wide
`LegacyGlyphs` object (`Player.kt:825-826`). Its `faces` is a strong
`IdentityHashMap<Font, HashMap<Int, Glyph>>` with no eviction or removal
(`LegacyGlyphs.kt:19-25`). Each draw inserts the font object, and each
nonbitmap glyph encountered adds its outline or missing-glyph entry to
that font's map (`:40-51`). The separate 128-entry image limit does not
bound either the font map or its outlines.

The cache outlives a reader. Every stop/start cycle creates a new reader
(`ExactActivity.kt:258-264`, `:500-510`), and an activity recreated in the
same process also adopts the running host with a new reader (`:68-101`).
Each reader's font announcements build fresh `Font` objects
(`ExactView.kt:65`, `:1095`; `Structure.kt:16-36`). Identity keys therefore
retain another copy of the same face's outlines after recreation. Neither
activity release nor view detachment removes those old keys. The cache also
keeps every distinct outline encountered while browsing text, regardless of
whether any retained drawing still uses it.

A long-running app on API 29 or 30 consequently retains font objects,
associated font buffers and outline paths for retired readers. Repeated
background/foreground cycles grow this history for the life of the process; an
active reader needs only its current faces. API 31 and newer use
`Canvas.drawGlyphs` and do not enter this fallback.

This finding is source-confirmed by the cache's complete ownership paths
and the activity replacement path. No Android device was launched, and no
heap-size or growth-rate measurement is claimed.

Make retired faces collectible or tie their caches to reader ownership,
and bound retained glyph data where browsing can grow it. Preserve safe
sharing between main-thread and worker drawing. Validate on API 29/30 by
recreating a reader repeatedly with the same font and by displaying then
removing many distinct glyphs; verify old faces become collectible and
retained cache memory reaches a bound. Keep the existing bounded image
cache covered as well.

Duplicate check: searched open and closed filesystem issues and all 275
cached GitHub issue titles and bodies for the legacy glyph implementation,
font retention, and glyph caches; no matching report was found.
