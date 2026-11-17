# SVG overprint

## Rough

The vocabulary excluded iOS and Android from `mix-blend-mode`, including artwork
whose backdrop was entirely inside one SVG. The parity smoke skipped that tile.

## Lean in

Keep the inks and their backdrop in one SVG. That bounds the native compositing
work and keeps the shapes available for hit testing.

## Needed

The iOS and tvOS presenter composites the SVG with Core Graphics; macOS keeps
Core Animation’s layer blend and outside backdrop. Android sends one
picture rendered by the shared CPU painter. Animations inside that picture,
including external sources referenced by a blended SVG, stay sampled. Their
samples invalidate each consuming scene and retained scroll row. Resolved masks
participate in that decision across SVGs. Raster bounds include shaped text and
shadows, intersect clip paths, and retain artwork beyond the window in scroll rows.
Square stroke caps remain inside the picture even at a low miter limit.
Blended leaves keep their opacity and clip. Android refuses blend pictures above
16,777,216 pixels before allocation, retaining the original unblended drawing.
Mask regions use the same pixel-area budget, retaining wide masked backdrops.
Optimized filter groups isolate their source even when the shadow is transparent.
Shadow offsets and blur follow device scale and SVG transforms. Text baselines
do not change animation ownership. If the picture cannot composite correctly,
it draws unblended and names the limit. The vocabulary names this route and
Canvas 2D's `globalCompositeOperation`.

## Checkpoints

The issue's multiply overprint is compared with a Chrome capture in the Apple
island test and Android recorder test. The gallery's blend tile participates in
native parity checks. Regression tests cover filtered updates, translated masks,
blended markers, route recordings, and motion reclaimed from the reader without
restarting its phase. They also sample shadows outside geometry, resolve blend
dependencies through use, marker, pattern and mask references, and keep every shape
when a blend picture exceeds the allocation limit. Prepared-path comparisons
require actual paths. Vertical shadows are checked at device scale 2 and under
a scaled viewBox. An asymmetric filtered island checks macOS image orientation;
text with a nondefault baseline keeps its sibling's live filtered animation.
Overprint beside a text label and clipped paths beyond the bitmap size limit
stay blended. Overflow checks assert the recorded picture's destination and diagonal square
caps. Cross-SVG animation checks cover ownership, Apple scene updates and Android
retained-row pixels. Apple leaves unsampled animations on the original layers.
