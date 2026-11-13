# A presence exit lands shared-element flights while enumerating them

**Status:** Open
**Systems:** host/apple
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`landFlights` iterates `flights.values` and `landFlight` removes that flight from the dictionary. Swift traps with "Dictionary mutated while being enumerated" on the next step. `flightsBatchApplied` already copies `Array(flights.values)` before it mutates.

A presence exit lands every flight whose slot sits inside the exiting view. One such flight is enough. The exit never finishes.

## Where

`host/apple/Sources/ExactKit/IOS/FlightsIOS.swift`, `landFlights` and `landFlight`. `PresenceIOS` calls `landFlights` from `beginExit`.

## How to reproduce

Start a shared-element flight, then exit a view that contains that flight's slot. The process traps in `landFlights`.

## Direction

Iterate `Array(flights.values)`, and land those whose slot is inside the exiting view.
