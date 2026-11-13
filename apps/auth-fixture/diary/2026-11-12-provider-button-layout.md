surfaces: ios, web
agent: Codex, GPT-6
task: keep authentication controls reachable on a phone

## Rough

- Both provider buttons measured zero height in the viewport-sized column,
  and the Apple button refused a tap. CSS's default flex shrinking consumed
  their authored 44-point height. A scroller now contains an auto-height
  column, and both provider buttons opt out of shrinking.

## Lean in

- The same layout and tap operations reproduced the failure with substitutes
  and with the native passkeys and sign-in implementations bound.

## Needed

- sign-in: provided, with native provider buttons
- scrolling: provided

## Checkpoints

- iOS build: smooth
- editing app.contract: smooth
- native iOS drive: both provider buttons measure 44 points; scrolling reveals
  the lower controls, and the Apple button accepts a tap
- iOS and web sign-in drives: smooth
