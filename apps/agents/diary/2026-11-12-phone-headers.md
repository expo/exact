surfaces: ios, macos, web
agent: Codex (GPT-6)
task: Keep header actions reachable at phone width.

### Rough

At 402 points under agent chrome, the chats search field kept its intrinsic width
and pushed Compose beyond the viewport. The field now takes the space left by the
heading and button, with `flex=1 min-width=0`. The sheet's Start button already fit
at this width.

Two existing 1100×720 web tests fail during sheet interaction with either header
layout. The phone navigation test settles each transition before the next press.

### Lean in

The driver reported the offscreen control's geometry and refused its tap. The
phone-width authored test reproduces that failure without a model or service.

### Needed

- navigation: provided

### Checkpoints

- iOS build: rough; a second build cleared a generated-input freshness refusal.
- Editing app.contract: smooth; only the heading's and search field's flex rows changed.
- Driving the app: the two phone tests pass on web and macOS, and under both iOS
  chrome modes. Under iOS agent chrome, Compose and Start end at x=386 in the 402-point
  viewport, and both taps succeed. The existing platform-chrome model drive passes.
