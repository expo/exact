# The iOS agent check squares each view's opacity

**Status:** Closed
**Resolution:** The coverage walk and the composited backing multiply each view's alpha once, since alpha is its layer's opacity.
**Systems:** host/apple
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

The coverage walk multiplies `UIView.alpha` by `layer.opacity` for every view, including ancestors. Setting `alpha` writes the same number into `layer.opacity`, so each view's opacity is applied twice. A node at opacity 0.5 is judged at 0.25, and every ancestor squares again. Contrast, the faded flag, and composited backing colors are wrong for any translucent view, so `check` can report a false failure or miss a real one.

## Where

`host/apple/Sources/ExactKit/IOS/AgentCheckIOS.swift`, `opacity` and `walk`.

## How to reproduce

Run the agent check on a view whose `alpha` is 0.5. The recorded coverage uses 0.25 for that view, and the product of the squares for its ancestors.

## Direction

Multiply only `view.alpha`, or only `layer.opacity`, once per view.
