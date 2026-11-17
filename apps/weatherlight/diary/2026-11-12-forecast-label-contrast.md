surfaces: web
agent: Codex, GPT-6
task: keep the forecast label legible over the sky

## Rough

The installed browser showed the solid fallback rather than the GPU sky, so the
existing contrast tests passed without exercising the reported backdrop. A browser
colour-compositing assertion reproduced the secondary label's 2.707:1 contrast on
an illuminated blue-grey backdrop. The primary label passes the same 3:1 requirement.
The actual animated sky still needs a run with a working graphics adapter.
On a Mac whose Chrome draws the sky, the high, low and feels-like line under the
condition measured 2.9:1 in its secondary colour at noon on 2 January, so it takes
the label colour as well.

## Lean in

The agent's virtual clock lets the browser regression select forecast hours and
sample animation times without sleeping.

## Needed

- none beyond views and data

## Checkpoints

- web build: smooth
- editing app.contract: smooth
- driving the app: rough; the graphics adapter did not draw the sky (see Rough)
