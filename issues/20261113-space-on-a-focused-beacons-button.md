# Space on a focused Beacons button does not activate it

**Status:** Open
**Systems:** game
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

Keydown ignores the edge and does not call `preventDefault` when the target is a button. Keyup always calls `preventDefault` for Space, E, and WASD. A button's click for Space is the keyup default action, so it never runs, and the sim does not take the edge either. Boot focuses Play. Space, the key the card names, starts nothing. Pause and Play again have the same hole once they take focus. Enter still activates the button. A click does too.

## Where

`game/twins/three/beacons/game.js`, the keydown and keyup listeners, and the focus of Play.

## How to reproduce

Open Beacons and press Space while Play is focused. The game stays on the title.

## Direction

Use the button-target exemption on keyup as well, and do not call `preventDefault` when the target is a button.
