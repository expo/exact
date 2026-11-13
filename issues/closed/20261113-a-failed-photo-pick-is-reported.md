# A failed photo pick is reported as a cancel

**Status:** Closed
**Resolution:** The photo picker hands the core why a chosen item did not stage, and the core refuses that pick with the reason logged instead of answering it as a cancel or a shorter list.
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`didFinishPicking` calls `done([])` for an empty result, and that path is a cancel. A non-empty result whose types are not an image or a movie, whose load returns no URL, or whose temp copy fails, leaves a nil slot. `compactMap` then calls the same `done([])` when every slot failed, and a shorter list when only some failed. The load error is ignored. The file input cannot tell a failed pick from Cancel, so the chosen photos never arrive and any previous value stays.

## Where

`host/apple/Sources/ExactMediaPicker/MediaPicker.swift`, `didFinishPicking`.

## How to reproduce

Pick a photo, and fail the temp copy (a full temporary directory is enough). The picker reports the same empty result as Cancel.

## Direction

Keep a failed stage distinct from cancel. Do not call `done([])` after the person picked results that did not stage.
