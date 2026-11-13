# A VNC disconnect leaves the middle and right buttons held

**Status:** Closed
**Resolution:** A session whose client disconnects holding the primary, middle or right button queues the cancel that releases the hold, not only for the primary.
**Systems:** host/linux
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

When a VNC client disconnects, held keyboard keys are released, and a held primary button sends `InputEvent::Cancel`, which becomes `pointer_lost`. A held middle or right button does not. Those buttons are RFB bits 2 and 4, delivered as `InputEvent::Aux` while the client is connected. The end of `serve` tests only `buttons & 1`.

An aux press with no other button held takes `pointer_held`. Without a cancel, that hold stays. Later pointer moves keep going to the node that was under the button, and the button mask stays down. The next client starts from a mask of zero, so it never sends the release.

## Where

`host/linux/src/vnc.rs`, the end of `serve`. `host/linux/src/presenter/contact.rs`, `pointer_lost`. `host/linux/src/presenter/pointer.rs`, `aux_pointer`.

## How to reproduce

Connect a VNC client, press the right or middle button on a node that takes a pointer hold, and disconnect without releasing it. Move the pointer from a new client, or from the local seat. The move still reports the old buttons, on the node that was held.

## Direction

When the reader ends, send `InputEvent::Cancel` whenever `buttons` is nonzero. `pointer_lost` already releases a hold taken by any button.
