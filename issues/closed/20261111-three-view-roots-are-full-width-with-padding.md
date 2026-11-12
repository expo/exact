# Three sources in the repository write a view's root `width="100%"` with padding and are wider than the window

**Status:** Closed
**Resolution:** Each of the three roots says box-sizing="border-box", so its width="100%" includes its padding and it is as wide as the window.
**Systems:** apps/motion-gallery/app.contract, apps/select-walk-fixture/app.contract, scripts/fixtures/prevent-focus.contract
**Severity:** P3
**Author:** Claude (Fable 5.2), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `d548892e`. `lower-width-padding` names a container written `width="100%"` with
horizontal padding or a border under the default `content-box`: it is wider than its
parent by the padding. Since `6082e9f2` the warning also names a view's root, whose
parent is the window. Compiling every tracked `.contract` file before and after that
commit, three sources gained the warning, each at its root:

- `apps/motion-gallery/app.contract:57`: `main testId="gallery" width="100%" height="100%" … padding=12`
- `apps/select-walk-fixture/app.contract:7`: `column testId="root" width="100%" padding=4 padding-top=240`
- `scripts/fixtures/prevent-focus.contract:17`: `column padding=20 gap=8 width="100%"`

Each root is wider than the window by twice its horizontal padding (24, 8 and 40
points), so its content can run past the window's right edge.

## The failure

`contract build apps/motion-gallery/app.contract -o /tmp/x.plan` prints

```
apps/motion-gallery/app.contract:57:5 warning [lower-width-padding] `width="100%"` with padding is wider than its parent by the padding …
```

and the other two print the same warning at the lines above. Expected: no warning,
and a root as wide as the window.

## Verification

`contract build` on each of the 763 tracked `.contract` files with the compiler built
at `09947777` and at `d548892e`; these three were the only sources whose diagnostics
changed, and no source went from compiling to refused. The layouts were not measured.

## Direction

Write `box-sizing="border-box"` on each root, or drop the `width` where the root is a
block that fills its parent anyway. The two fixtures are driven by tests that may pin
positions (`select-walk-fixture`, `prevent-focus`): run their tests after the edit and
move a pinned number only where the old one came from the overflow.

## Related

`issues/closed/20261110-width-padding-warning-skips-the-view-root.md`, whose fix made
the warning see these roots.
