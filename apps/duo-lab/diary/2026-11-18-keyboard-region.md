surfaces: none yet (laid out headlessly by `contract/cli/tests/it/duo_lab.rs`)
agent: Claude (Opus 5.5)
task: A keyboard-region screen for the lab, shown before any host reports the keyboard.

### Rough

No host reports the keyboard to the kernel until LLP 1123's later stages, so the screen cannot run on a phone yet; it waits under `fixtures/` and joins the stack when hosts report the keyboard.

### Lean in

Each column states on screen where it ends, with the keyboard up and down, so the screen explains itself on a device, and the headless test checks exactly those statements.

### Needed

- Contract names for `-exact-safe-area-ignore` and the reservations (LLP 1123 D4, D11): provided
- the keyboard region in the kernel (LLP 1123 D7, expo/exact#902): provided
- the iOS host reporting the keyboard (LLP 1123 stage 3): not yet

### Checkpoints

- `cargo test -p contract --test it duo_lab` lays the screen out as an iPhone 18 Pro with the keyboard down, up and dismissed again, and each column ends where its words say.
