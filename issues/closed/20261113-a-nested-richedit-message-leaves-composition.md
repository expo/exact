# A nested RichEdit message leaves composition stuck

**Status:** Closed
**Resolution:** A composition end that arrives while the hook holds the fields is kept and applied after the outer dispatch, and a start whose dispatch fails begins no composition, so composing never outlives its end.
**Systems:** host/windows
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

RichEdit handles an IME message while it holds the mutable borrow of every field. A nested IME callback cannot take that borrow, so `message` returns false and leaves the message to Windows. `composing` is set true on `WM_IME_STARTCOMPOSITION` before `dispatch`. It is cleared only when this same call handles `WM_IME_ENDCOMPOSITION` after `dispatch` returns `Ok`.

An end that arrives nested, or a `dispatch` that returns `Err` after the start flag was set, leaves `composing` true. While it is true, Tab and Enter are dispatched into the editor instead of being returned to the app. The flag stays true for later keys.

## Where

`host/ink/src/device/d3d11/fields.rs`, `message`.

## How to reproduce

Focus a RichEdit field and start a composition whose messages re-enter the hook before `WM_IME_ENDCOMPOSITION` is handled by the outer call (the comment on the borrow says synchronous IME messages nest). After the composition ends, press Tab or Enter. The key goes to the editor. The app's key handling, which runs only when `composing` is false, does not see it.

## Direction

Do not leave `composing` set when this call did not finish the composition. A nested `WM_IME_ENDCOMPOSITION` that cannot borrow should be recorded and applied when the outer call releases the borrow. A `dispatch` error after setting the flag should clear it unless this message is a start that the control accepted.
