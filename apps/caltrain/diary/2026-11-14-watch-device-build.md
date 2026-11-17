surfaces: ios, watchos
agent: Claude Opus 5.5
task: embed the watch app in Caltrain's device builds and archives (#489)

### Rough

No signing identity or provisioning profile is installed on the Mac this was
built on, so only the ad-hoc signed path (`--unsigned`) was built. Nothing was
installed on an iPhone or an Apple Watch.

Caltrain cannot make a distribution archive: `--archive` without
`--development` refuses before building because the app declares no 1024 px
icon (`host.ios.icon` or a square icon). The production watch bundle was
checked with `--device --unsigned --release` instead, which is the same build
without the icon requirement.

A cold development device archive took 49 minutes on a heavily loaded machine:
two Rust targets, `aarch64-apple-ios` and `aarch64-apple-watchos` with
`-Zbuild-std`, and two Swift scratches.

### Lean in

The watch app goes through `embedWatch` for both the simulator and a device. The
device build changes only the target, triple, SDK, entitlements and signing.

### Needed

- a watch app on a device: built here; run nowhere yet.
- the phone delivering new watch plans: not built. The watch app runs the plan
  it was built with (LLP 1126.002 D10).

### Checkpoints

- `--watchos --no-install`: the watch app builds at the watchOS 26 floor and
  runs live on a Series 12 simulator (departures shown).
- `--device --archive … --unsigned --development`: `Payload/Caltrain.app/Watch/Caltrain.app`
  with its complications; `codesign --verify --deep --strict` passes on both
  bundles; every binary is arm64 alone, platform watchOS, minos 26.0.
- `--device --unsigned --release`: DT keys from the watchOS SDK, stripped
  binaries, and dSYMs with matching UUIDs in `Caltrain.dSYMs/Watch/`.
