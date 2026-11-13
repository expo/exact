# A display link with a zero period traps the frame sampler

**Status:** Open
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`observe` stores `period` as `(target - now) * 1000`. The next callback samples with that stored period. `sample` computes `Int((interval / p).rounded())`. A callback whose `targetTimestamp` equals `timestamp` stores 0. The next callback divides by zero. The ratio is infinite or NaN, and converting that to `Int` traps, which aborts the process. Development builds sample from the first activity, and a recording does too.

## Where

`host/apple/Sources/ExactKit/FrameSampler.swift`, `observe` and `sample`.

## How to reproduce

A `CADisplayLink` callback with `targetTimestamp == timestamp`, then one more callback. The process traps in `sample`.

## Direction

Sample only when the announced period is finite and greater than zero. Do not convert a non-finite `Double` to `Int`.
