# Releasing one barcode group ends the shared scan

**Status:** Open
**Systems:** packages/barcode-detection
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A live scan is keyed by track and formats. `release` of one argument group for that key calls `end` or `endScan`, which drops the observer and `seen`. Another reader with a different interval, or the same formats in a different array order, is a different argument group and still reads that scan.

Android refcounts those groups, including interval and the original format-array order, and ends the scan only when the last group releases. After the in-flight poll returns, the runner releases the departed group. The next Apple or web poll builds a new scan with empty `seen` and a gap in observation.

## Where

`packages/barcode-detection/apple/BarcodeDetection.swift`, `release`. `packages/barcode-detection/web/index.js`, `releaseArgs`. `packages/barcode-detection/android/BarcodeDetection.kt`, `release` and `argumentGroup`.

## How to reproduce

Read `detectedBarcodes` twice on the same track and formats, with two intervals. Release one reader. On Apple and the web the scan stops and `seen` is cleared. On Android the other reader keeps the scan.

## Direction

Refcount groups as Android does, including interval and the original format-array order. End the scan only when the last group for that track and format set releases.
