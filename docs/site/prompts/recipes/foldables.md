```prompt Lay out my app for a foldable phone
Make the Exact app in this folder lay out its panes by the fold of a foldable: a book fold or a laptop fold. Use https://github.com/expo/exact/blob/main/docs/recipes/foldables.md as the source for each step. The Exact checkout has the same page at docs/recipes/foldables.md.

1. If this folder is not an Exact app (it has no exact.mjs and app.contract), stop and tell me.
2. If I did not say which screens change on a fold, and what each segment holds in a book fold and in a laptop fold, propose it and ask me before you write code.
3. In app.contract, read `exactViewport()` as a resource, with a shape that has the fields you use. Derive the fold from them: `horizontalViewportSegments=2` is a book fold, and `verticalViewportSegments=2` is a laptop fold.
4. Place a box in segment `x, y` with `env(viewport-segment-<left|top|width|height> <x> <y>)`, and write the layout without a fold in the same attribute, as the recipe's `Reply` shows. Segment coordinates are relative to the viewport: keep the pane container at that origin, and chrome inside a pane.
5. Keep controls off the hinge; scrolling content can span it. A pane that does not scroll keeps its width: centred, or on the trailing segment of a book fold. In a laptop fold, put authored controls on the bottom segment. Put the first pane on the leading segment: indices count from the left, so that is segment 1 in RTL.
6. Keep the same pane instances when the fold changes, so a field keeps its text. For a list and its detail, use a split view (`pattern list-detail`), not panes of your own.
7. Add a test in app.test.contract that ends on each screen you changed and expects its controls without a fold. The recipe names no test step for a fold: do not invent one.
8. Follow the platform checks in docs/start-here.md (`bun exact.mjs docs platform checks`) for the targets in the app's AGENTS.md and app.json until they pass. Read the reports, including counted findings; a zero exit code alone does not mean clean. Fix the app, not the test. If a finding stays after three rounds of fixes, or the app had it before you changed anything, record it (step 9) and report it; do not keep trying.
9. When a doc was wrong or missing, or a step needed a workaround, read `bun exact.mjs feedback status`. Under `never`, keep no note. Otherwise record it: `bun exact.mjs feedback note "<what happened>"`, which goes to Expo only as that status says. Do not change the policy; tell me how many notes you kept.
10. Tell me which screens change and what each segment holds in each fold, what the tests and checks reported, and that I must look at both folds by hand on a native Duo host. A web page needs a browser that supports these signals: a feature flag alone does not establish that.
```
