# A modified CC-BY model and WPT's URL test data are committed with no licence file and no NOTICE entry

**Status:** Closed
**Resolution:** The Khronos sample's `Fox-LICENSE.md` and a README crediting each author and listing the changes now sit beside `fox.glb`, WPT's `LICENSE.md` (3-Clause BSD) beside `urltestdata.json`, NOTICE credits both, and `samples.rs` credits the Fox as its metadata does.
**Systems:** NOTICE, game/games/skinned-fixture/art, vendor/ibex/third_party/wpt
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

`NOTICE` at `dd6f5dcd` lists the code under `vendor/`, one person's images, the fonts and one ported app, and says the licences sit beside the files. Two files are someone else's work, have no licence file beside them and are not in NOTICE.

## The Fox model

`game/games/skinned-fixture/art/fox.glb` is the Khronos glTF sample Fox. Its own `asset.copyright` field reads:

```
CC-BY 4.0 Model by PixelMannen https://opengameart.org/content/fox-and-shiba and @tomkranis https://sketchfab.com/3d-models/low-poly-fox-by-pixelmannen-animated-371dea88d7e04a76af5763f2a36866bc and @AsoboStudio with @scurest https://github.com/KhronosGroup/glTF-Sample-Models/pull/150#issuecomment-406300118
```

So the credit survives inside the binary. Two things CC BY 4.0 section 3(a)(1) asks for do not appear anywhere (https://creativecommons.org/licenses/by/4.0/legalcode): the licence's text or link, and an indication that the material was modified. This copy is modified. Against the sample (162,852 bytes) it is 163,944 bytes and adds `asset.extras.metersPerUnit`, `extras.markers` on two animations, one channel and sampler on each of those, four accessors and four buffer views.

The five unmodified copies under `experiments/game-engine-comparison/full/*/assets/` each have the sample's `Fox-LICENSE.md` beside them, which covers "all files in this directory tree". `game/bake/tests/samples.rs:1-2` says the Khronos samples are "fetched into an external cache, never vendored", and credits the Fox as "PixelMannen, CC0; conversion by @tommy3b", where the file's field says CC-BY 4.0 and `Fox-LICENSE.md` lists CC0-1.0 and CC-BY-4.0.

## The WPT data

`vendor/ibex/third_party/wpt/urltestdata.json` (228,685 bytes) is web-platform-tests' `url/resources/urltestdata.json`, as `vendor/ibex/crates/ibex2/tests/wpt_url.rs:1` says. WPT is under the 3-Clause BSD licence, whose first condition is that redistributions of source keep the copyright notice, the conditions and the disclaimer. The directory holds only the JSON. `vendor/ibex/LICENSE` is Ibex's MIT licence, NOTICE:6 lists "ibex (MIT)", and the string "web-platform-tests" is in no tracked file.

## Verification

Read the JSON chunk of `fox.glb` and of the experiments' copies, and compared the two documents key by key and the binary chunks by hash. Searched the tracked text files for the credited names: outside `experiments/` the only hit is `game/bake/tests/samples.rs:2`. Searched for "web-platform-tests": no hit. Listed `vendor/ibex/third_party/` (one file). Not checked: whether the baked `fox.model` keeps the copyright string.

## Direction

Put `Fox-LICENSE.md` and a line stating the changes beside `fox.glb`, and make the comment in `samples.rs` agree with it. Put WPT's `LICENSE.md` beside `urltestdata.json`. Add both to NOTICE.

## Related

A separate issue covers NOTICE's credit for the gallery pictures. No existing issue or QUEUE entry mentions these files' licences.
