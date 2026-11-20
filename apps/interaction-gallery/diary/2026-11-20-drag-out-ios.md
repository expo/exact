surfaces: ios
agent: Claude Code (Claude Opus 5.5)
task: a study's card lifts into another app on iOS and iPadOS, LLP 1094.000 stage 2

The Photos cards needed no change: the declaration stage 1 added now lifts on iOS and iPadOS. A press held on
a card lifts it as itself, and the photograph drops into Safari, Files, Photos and Messages as the shipped
PNG, the same bytes.

### Rough

- A drive (`agent.mjs ios`) draws the app itself and installs no drag interaction, as it installs no context
  menu, so the lift is seen only in the app run as a person runs it.
- The iOS 27.1 simulator runtime, the only one with the iPhone Duo, has Safari and Settings but no Files,
  Photos or Messages; the Duo's drops went to a page in Safari.

### Lean in

- Nothing to write beyond stage 1's attribute pair.

### Needed

- Nothing further for iOS; the Mac, the web and Android lift the card in LLP 1094.000's later stages.

### Checkpoints

- iOS 27.1, iPhone Duo simulator: the authored tests, 6 of 6; a card dropped into Safari's page.
- iOS 27.0, iPhone 18 Pro simulator: a card dropped into Safari, Files, Photos and a Messages conversation.
- iPadOS 27.0, iPad Pro 11-inch simulator: a card dropped into Safari.
