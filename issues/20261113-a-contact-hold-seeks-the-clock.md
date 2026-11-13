# A contact hold seeks the clock past canvas sensor samples

**Status:** Open
**Systems:** host/linux
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

A contact `hold` seeks the presenter clock by the hold's duration, up to 10 s. That seek is `Presenter::clock`, which advances the host and does not post canvas sensor samples. The agent `clock` operation uses `clock_framed`, which posts one display frame of samples and then steps the world, because one jump would queue the whole span past the world's 1,024-event bound.

The world therefore runs the hold with no accelerometer or gyro samples for that interval. A later `clock` posts the gap at once. The input queue keeps 1,024 events and drops the oldest. Two 60 Hz feeds across a hold of about 9 s are enough to drop the start of the interval.

## Where

`host/linux/src/agent/contact.rs`, the `hold` arm. `host/linux/src/presenter/clock.rs`, `clock`. `host/linux/src/agent/sensors.rs`, `clock_framed`. `host/linux/src/agent.rs`, the clock operation. `game/engine/src/sim/queue.rs`, the overflow that drops the oldest event.

## How to reproduce

Drive a canvas that has a sensor feed. Hold a contact for several seconds, then read `state` and advance `clock` across the same span. The hold's world steps have no samples from that interval. The later clock either bunches them into one tick or drops the oldest once the queue is full.

## Direction

Seek a hold through `clock_framed` before returning, so each frame's samples are posted before the world steps. Do not seek a world across host time that still has unposted sensor samples.
