# Recipe: porting an app, a release build, splitting the work

**Name the port when you make it:** `exact new <path> --name "Euxy" --bundle-id com.yourcompany.euxy`,
with the original's name and bundle id if it replaces the original on a device. Changing the bundle id makes the next build cold and leaves the old app installed.
Change app.json's `app.id` and any `appId` exported by app.ts together. An app ships the platforms app.json names: a native verb names
its own on its first run, and `bun exact.mjs update --without <platform>` stops shipping one
(LLP 1153 D1). A port that ran `linux` once hears each `symbol:sf/…` warn that Linux draws it
blank until `update --without linux`; one that ran `mac` hears `tamber-unreachable` name a Mac
control that ignores the design (NSSlider's accent) until `update --without macos`. The SF
Symbols the web draws blank are one warning line.

**Comparing a port with the original.** `agent ios --touch platform` makes each tap a real touch,
opening UIKit menus and native module controls to compare screens with the original ([driving iOS](ios-driving.md)). Porting a React Native app,
read [its pitfalls](../agent-pitfalls.md#porting-a-react-native-app) first.

**What ships.** `ios --release` builds what ships; it ignores `EXACT_AGENT`, so drive the
development build. The first takes two minutes: start `--prebuild --release`; later
`ios --release` waits for a build still running, then takes seconds.
Apple bundles declare Exact's required-reason APIs. Add app disclosures in
`PrivacyInfo.xcprivacy` beside `app.json` ([privacy manifests](../reference.md#apple-privacy-manifests)).
A `--touch platform` drive turns the simulator's accessibility on while it runs (an app launched
with it on loads UIKit's accessibility runtime: 1.4 s) and restores what it found at its end. A
launch that slow (`main→didFinishLaunching` over a second) had it on before; turn it off:

```sh
for k in AutomationEnabled AccessibilityEnabled ApplicationAccessibilityEnabled; do xcrun simctl spawn <udid> defaults delete com.apple.Accessibility $k; done
```

Time the installed build, not a plan swapped in by `agent ios`: `bun exact.mjs
ios` each variant first. `SIMCTL_CHILD_EXACT_SMOKE=1 xcrun simctl launch --console-pty <udid>
<bundle id>` prints Exact's phases to first draw (exec→main, scene, runner and layout, apply,
painted) and exits; a data module's first answer is a hook's to stamp.

**Contract's sampler, MIDI and transport** ([guide](../contract-for-agents.md#sound-midi-and-media)):
do not replace them in a native module. Continuous or synthesized sound Contract lacks may use
[a GPU module's output](../../gpu/README.md#a-modules-own-audio-output), with session and
lifecycle handling and no Contract sound journal.

**Splitting a port among helpers:** commit the seam between the halves first (the shared types,
the native module's interface), then start each helper in the background from that commit,
owning its half and stubbing none of another's. Keep test screens off branches you merge, and keep working.
