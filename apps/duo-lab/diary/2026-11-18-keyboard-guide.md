surfaces: none yet (laid out headlessly by `contract/cli/tests/it/duo_lab.rs`)
agent: Claude (Opus 5.5)
task: A keyboard-guide screen for the lab: a composer that reads the keyboard under `overlays-content`.

### Rough

As with the keyboard-region screen, no host reports the keyboard yet, so the screen waits under `fixtures/` and joins the stack in LLP 1123's stage 5.

UIKit's default guide, `max(var(--exact-keyboard-inset-bottom), var(--exact-safe-area-inset-bottom))`, does not give UIKit's value yet: `var(--exact-safe-area-inset-*)` still reads the content owner's inset, 0 at a content root the host clears, not the box's own observation, which needs the hosts to report their bands and comes with LLP 1123's activation. So the composer stays above the home indicator and pads by the keyboard's reading alone; after activation it can reach the screen's bottom and pad by the composed guide.

### Lean in

The composer needs no keyboard property: one length, its bottom padding, places its field on the keyboard.

### Needed

- the keyboard's readings in the kernel and Contract (LLP 1123 D7): provided
- `var(--exact-safe-area-inset-*)` as the box's own observation (LLP 1123's activation): not yet
- the iOS host reporting the keyboard (LLP 1123 stage 3): not yet

### Checkpoints

- `cargo test -p contract --test it duo_lab` lays the screen out as an iPhone 18 Pro: the composer ends above the home indicator, and its field ends there with the keyboard down and on the keyboard's top with it up.
