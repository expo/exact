# A picture-in-picture refusal can land on the next plan

**Status:** Open
**Systems:** host/web
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

`requestPictureInPicture` on the wasm page logs a refusal and schedules `exact_dispatch` of `pictureinpictureerror` on a timer, using the element's view id. The timer does not capture the incarnation. `format` and `openURL` do, and they skip the dispatch once the incarnation has moved.

A reload tears the page down and bumps `host.incarnation` before that timer runs. The boot comment records that view ids can be reused. The timer then reads `dataset.view` and dispatches into the new plan, so the new view hears `pictureinpictureerror` for a request it did not make.

## Where

`host/web/wasm/commit.js`, the `requestPictureInPicture` arm, and the incarnation checks on `format` and `openURL`. `host/web/wasm/boot.js`, where the incarnation increments.

## How to reproduce

On the wasm dev page, request picture-in-picture and let a plan reload finish before the refusal timer. The reloaded view that reused the id receives `pictureinpictureerror`.

## Direction

Capture the incarnation and the element. Dispatch only when the incarnation still matches, `views.get(id)` is that element, and the element is not retired.
