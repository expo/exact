# Releasing one Health source stops the other observers

**Status:** Open
**Systems:** packages/health
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`release` of any source other than `healthStatus` stops every `HKObserverQuery` and clears the map. `healthStatistics` and `healthWorkouts` are separate live sources and share that map. Each source starts the observer for the types it reads.

The runner releases a source when its last reader goes. Unmounting a steps chart while a workout list remains stops the workout observer. Later samples are not announced until the app is foregrounded again or that source is read from scratch.

## Where

`packages/health/apple/Health.swift`, `release` and `observe`.

## How to reproduce

Read `healthStatistics` and `healthWorkouts` together. Drop the statistics reader. A new workout sample does not announce `healthWorkouts` until the next foreground or a fresh read.

## Direction

Stop only the queries that source started, or refcount them. Leave an observer that another live source still needs.
