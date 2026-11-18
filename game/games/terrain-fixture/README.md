# Terrain fixture

This fixture uses a seeded heightfield and a fixed camera (LLP 1182.000 D4a).
The hook meshes up to 64 of the window's 2,312 section addresses per advancing
frame and submits one indexed draw per visible section with exposed terrain.
The logic test counts those sections independently by testing each section's
eight corners.

Check the recorded tick and save baseline at 1280 × 720 from the repository root.

```sh
bun game/games/terrain-fixture/proof.mjs web
bun game/games/terrain-fixture/proof.mjs linux
```

Each leg runs the drive twice. Settling at the play timestamp validates the
pipeline with all 2,312 sections still pending. The proof takes 20 extra looks
partway through meshing and requires presentation after exactly 37 advances.
It compares pixels at the same partial and complete frames across both runs.
After a perf reset and one advance, every sample must report the pinned 823
sections.

The Linux leg enables the GPU-backed agent and reads the bake's shader pack.
On a Mac with Metal and Chrome with WebGPU, record the tick and save baseline
again with `bun game/prove.mjs terrain-fixture --hosts linux,web --repin`.
