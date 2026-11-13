# A discovered Shelf book is stored as 300 pages

**Status:** Open
**Systems:** apps/shelf
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

Discover stores every Open Library work as 300 pages. The subjects response has no page count, and the comment says the person can edit the default. Adding the work saves that number. The book screen prints Pages and has no control that changes them.

Marking the book done sets the current page to `pages`. Reaching `pages` marks the book done. Finishing a discovered book jumps to page 300. Percent and pages read count those pages. A shorter book cannot be finished on its real last page.

## Where

`apps/shelf/app.ts`, `subjectWorks` and the `done` branch of the book update. `apps/shelf/app.contract`, `addWork` and the book screen's Pages row.

## How to reproduce

Add a book from Discover and mark it done. The progress shows page 300 of 300. There is no field for the real length.

## Direction

Add the book with pages empty, or offer a pages editor on the book. Do not treat the placeholder length as the last page.
