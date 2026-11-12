# The evdev reader treats a touchpad as a touch screen: a finger on the pad jumps the pointer and presses

**Status:** Closed
**Resolution:** The evdev reader tells a touchpad by its fingers, no pen and no INPUT_PROP_DIRECT, moves the pointer by the finger's travel, and presses only on its click (BTN_LEFT), never on BTN_TOUCH.
**Systems:** host/linux/src/input.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`Input::open` classes a device by its event codes alone. Any device with `ABS_X` and
`ABS_Y` is an absolute pointer, kept with its axis ranges
(`host/linux/src/input.rs:158-169`). `Input::read` then maps every `ABS_X`, `ABS_Y`,
`ABS_MT_POSITION_X` and `ABS_MT_POSITION_Y` to a fraction of the screen
(`input.rs:316-327`) and turns `BTN_TOUCH` into the primary button for every device
(`input.rs:354`).

A touchpad reports the same codes as a touch screen. The kernel's event-code
documentation (<https://docs.kernel.org/input/event-codes.html>, "Guidelines"):
"Trackpads that provide absolute touch position must report ABS_{X,Y} for the location
of the touch. BTN_TOUCH should be used to report when a touch is active on the
trackpad." It tells the two apart by a device property: `INPUT_PROP_DIRECT` for a
screen, `INPUT_PROP_POINTER` for a pad, whose coordinates "require non-trivial
transformation, such as absolute to relative transformation for touchpads" ("Device
properties"). The reader never looks at the properties.

The host elsewhere counts a touchpad as a pointer with a cursor. `touch_only`
(`presenter/device.rs:175-182`) takes a device that is absolute, reports touch and has
a left button as one, and its test names that device `touchpad`
(`device_tests.rs:341-359`). `docs/contract-for-agents.md:3267-3270` tells an app
author that `exactViewport()` is `fine` and `hover` on Linux unless a touch screen is
there "with no mouse or touchpad".

## The failure

A laptop with a touchpad and no mouse, running the KMS host. By the code, resting a
finger in the middle of the pad does this:

- The position arrives: the pointer jumps to the middle of the screen
  (`display/pointer.rs:35-44`).
- `BTN_TOUCH 1` arrives: `Button(true)`, a press there.
- Lifting the finger: `Button(false)`, the release that completes a click.

The pointer cannot be moved without pressing, every touch clicks what is under the
pad-mapped point, and the app is told `hover: hover`. Expected: a finger's motion on
the pad moves the pointer by its change, and only `BTN_LEFT` (the pad's click)
presses.

## Verification

Read, not run: `input.rs` compiles only on Linux, this review ran on a Mac, and
LLP 1015 §6 records that evdev has carried no real event yet. The path is the lines
cited above; which codes a touchpad sends is from the kernel's documentation, not
observed.

## Direction

Read the device's properties at open (`Device::properties()`, `PropType::POINTER`).
For such a device, turn each `ABS_X` and `ABS_Y` report into a `Motion` by its change
from the last one, starting again when `BTN_TOUCH` goes down, and do not press on
`BTN_TOUCH`.

## Related

LLP 1015 §7 declares libinput, with pointer acceleration and touchpad gestures, out
of v1, and `QUEUE.md`'s "Linux host (LLP 1015 §7)" entry repeats it. That is
tap-to-click, two-finger scroll and acceleration, not pointing. `QUEUE.md`, "Run the
Linux host on KMS" asks for evdev to be qualified on hardware and names no touchpad.
