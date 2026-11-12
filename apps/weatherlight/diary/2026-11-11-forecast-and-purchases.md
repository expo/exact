surfaces: macos, ios, web
agent: Codex, GPT-6
task: Restore forecast loading, purchases and the fifth-day review request

## Rough

- The forecast parser rejected the whole response when an unused later hour
  or a daily probability or UV reading was null. Controlled HTTP 200 responses
  reproduced the empty forecast. Parsing now checks the displayed hours and
  keeps unavailable daily probability and UV readings as missing values.
- The package file resolver required an existing regular file. Purchases
  therefore could not create its synthetic store, and store review could not
  save its request journal. The shared walk now accepts missing destinations
  while refusing links in every existing component and directory destinations.
  Compose checks that attachments exist before presenting a composer.
- The purchase drives depended on the weather service. Their native runs now
  bake a response fixture into the app's data module, retaining the parser,
  purchase delivery, storage and review tasks. Outside HTTPS requests fail.

## Lean in

- The baked-bytecode tests reproduced the parser failures without a service.
- The saved review mark refreshes the visit resource, so losing and recovering
  a forecast cannot rearm the review task in the same session.

## Needed

- purchases and store review: provided
- app files: provided, with destination resolution repaired

## Checkpoints

- Reproduction: rough; see Rough.
- Bytecode regression tests: smooth.
- Package file-walk tests: smooth.
- Missing Compose attachments: refused before presenting.
- macOS and iOS purchase, relaunch and fifth-day review drives: smooth.
- tvOS compilation: smooth.
- Web: the client build runs. JS prerendering fails in geocoding because
  `structuredClone` is missing, and two contrast tests fail on the selected
  forecast label. Both failures reproduce with the baseline app sources.
