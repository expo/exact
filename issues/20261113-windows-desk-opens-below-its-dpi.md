# Windows Desk opens below its DPI-scaled minimum

**Status:** Open
**Systems:** apps/windows-desk
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`CreateWindowExW` uses a fixed 1040 by 860 physical pixels. After per-monitor v2 awareness, `WM_GETMINMAXINFO` scales the minimum track size by `GetDpiForWindow / 96` from 760 by 710. Above about 125% the window is created smaller than its own minimum. Windows applies that minimum when the person resizes, not at creation.

At 150% the minimum is 1140 by 1065. On a 1080p work area that height does not fit, so the controls that landed outside the client cannot be resized into view.

## Where

`apps/windows-desk/src/native.rs`: `WM_GETMINMAXINFO` scales `ptMinTrackSize`; `CreateWindowExW` passes 1040 and 860.

## How to reproduce

Launch Windows Desk at 150% DPI on a 1080p display. The window is shorter than the minimum it will later enforce, and the lower controls sit outside the client.

## Direction

Scale the initial width and height by the monitor DPI the same way the minimum is scaled, and choose a minimum the work area can show.
