# Recipe: a 3D model viewer

Run `bun exact.mjs add <exact2>/packages/model-viewer`.
Copy [assembly.glb](../../packages/model-viewer/test/fixture/assets/assembly.glb)
into your app's `assets/`. For a new app, replace its starter `app.ts` with
[this empty data module](../../packages/fixtures/sky/test/fixture/app.ts).
Write:

```contract
use ModelNode, ModelHotspot, ModelViewerState from "@exact/model-viewer"

component App
  state picked = ""
  state orbit = "0deg 90deg 105%"
  state clipped = false
  resource viewer = exactSurface("modelViewer") as shape ModelViewerState
  action pick(name: string)
    picked = name
  action front
    orbit = "0deg 90deg 105%"
    postMessage("reset", "modelViewer")
  action top
    orbit = "0deg 0deg 105%"
    postMessage("reset", "modelViewer")
  action reset
    postMessage("reset", "modelViewer")
  action toggleClip
    clipped = not clipped
  view
    column gap=8 padding=8
      canvas surface=modelViewer(src="assets/assembly.glb", cameraControls=true, cameraOrbit=orbit, cameraTarget="0m 0m 0m", fieldOfView="30deg", nodes=[ModelNode(name="Housing", color="", opacity=1, visibility="visible", occlusion="occluder", occludedOpacity=0.3), ModelNode(name="Impeller", color="#0080ff", opacity=1, visibility="visible", occlusion="revealed", occludedOpacity=0.3)], hotspots=[ModelHotspot(child="inlet", node="Inlet", position="0m 0.8m 0.85m", normal="0 0 1")], clipPosition="0m 0m 0m", clipNormal=(clipped ? "0 0 -1" : "")) message=pick width=320 height=320 flex-shrink=0 role="group" aria-label="Pump assembly" testId="pump"
        if not viewer.presented
          text "Loading model" position="absolute" inset=0 aria-busy=true testId="loading"
        button "Inlet" press=pick("Inlet") position="absolute" testId="inlet"
      row gap=8
        button "Front" press=front testId="front"
        button "Top" press=top testId="top"
        button "Reset" press=reset testId="reset"
        button "Clip" press=toggleClip testId="clip"
      text picked testId="picked"
      text viewer.error testId="error"
      text viewer.cameraOrbit testId="camera-orbit" font-variant-numeric="tabular-nums"
```

`cameraOrbit` is azimuth, polar angle, radius; `105%` frames with a margin.
`nodes` style subtrees: `revealed` parts fade behind opaque `occluder` shells.
For a translucent shell, use `occlusion="none"` and `opacity` below 1.

Hotspots match direct children by `testId`; hidden anchors hide their labels.
Picks return a mesh node name, or `""`.

Bind a downloaded GLB's `app:/data/`, `app:/cache/` or `app:/tmp/` path
as `src`; direct URLs are refused. Meshopt and quantization are supported.

`viewer` gives loading, errors, nodes and the settled camera to save.
Its record is the first live viewer's. Controls reserve touches and wheels
at mount; leave room beside the canvas to scroll.

Build with `bun exact.mjs web-build` or `EXACT_IDENTITY=- bun exact.mjs mac`;
run `bun exact.mjs agent web "clock settle" state` (or `macos`).
See [values and limits](../contract-for-agents.md#model-viewer).
