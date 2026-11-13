# Web motion drops an explicit sensor frequency

**Status:** Open
**Systems:** packages/motion
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`rate` is null when any reader group has no frequency above zero. `startSensorInput` calls `stream(name, {})`, which inserts that group. A reader that omits frequency does the same. The Generic Sensor is then constructed with no frequency hint.

Apple and Android take the max of each reader's rate, using 60 Hz or 50 Hz when a reader names none, and each canvas input's rate. On Chromium a 200 Hz canvas input, or a 200 Hz reader beside one that named none, stays at the browser default.

## Where

`packages/motion/web/index.js`, `group`, `stream`, `rate`, and `start`.

## How to reproduce

Feed a canvas sensor input at 200 Hz, or read a sensor at 200 Hz beside a reader that omits `frequency`. The Generic Sensor is started with `{}`. Samples arrive at the browser default.

## Direction

Do not insert a reader group for a canvas input. Max the frequencies that are greater than zero, and use 60 Hz where a reader named none.
