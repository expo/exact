# A `switch` in an SVG image draws its first child even when that child's `systemLanguage` does not match

**Status:** Closed
**Resolution:** SVG images with no user languages skip systemLanguage-conditioned elements and select an unconditional switch fallback.
**Systems:** host/svg-file/src/lower.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

LLP 1011.001 D2 reads a file's `switch` as "taking the first child whose conditions
pass". The reader tests two conditions: a child with `requiredExtensions` or
`requiredFeatures` is skipped, and the first other child is taken
(`host/svg-file/src/lower.rs:283-300`). The third condition, `systemLanguage`, is in
the list of attributes the reader ignores (`lower.rs:459-478`), so a child that
carries it always passes.

A picture with a label in several languages is written this way: one child for each
language, then a child with no condition as the fallback. Here it always draws the
first language listed, whatever the user's languages are, and never the fallback.
Outside a `switch` the attribute is ignored too, so an element a browser hides for a
reader of another language is drawn.

## The failure

```xml
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 40">
  <switch>
    <text systemLanguage="de" y="20">Hallo</text>
    <text systemLanguage="fr" y="20">Bonjour</text>
    <text y="20">Hello</text>
  </switch>
</svg>
```

Observed: the scene of `exact_svg_file::load(file)` holds the run `Hallo` and no
other, with no warning.

Expected, for a user whose languages include neither German nor French: `Hello`.
[SVG 2 §5.6.3](https://www.w3.org/TR/SVG2/struct.html#SwitchElement): a `switch`
"processes and renders the first child for which these attributes evaluate to true";
[§5.6.5](https://www.w3.org/TR/SVG2/struct.html#ConditionalProcessingSystemLanguageAttribute):
`systemLanguage` is true only when one of its tags matches one of the user's
languages.

## Verification

Run:

- A temporary test in `host/svg-file/tests` loaded the file above and searched the
  scene's debug form: `Hallo` present, `Bonjour` and `Hello` absent. With rectangles
  in place of the text (`systemLanguage="fr"` red, then an unconditional blue), the
  scene's one shape was the red one; with `requiredExtensions` on the first, the blue
  one. A red rectangle with `systemLanguage="fr"` outside any `switch` was drawn.
- Headless Chrome 156 drew the same rectangle files as images into a canvas: blue
  for the `switch`, and nothing for the red rectangle outside a `switch`. That
  headless build matched no `systemLanguage` value, `en-US` included, so it shows
  that a child whose test fails is skipped, not which language a user gets.

## Direction

Evaluate `systemLanguage` where `requiredExtensions` is tested, against the user's
languages, which `load` would take from the host. Without them, skip a child that
carries the attribute, so the unconditional fallback draws.

## Related

LLP 1055.000 §7 leaves `switch` out of the inline `svg` "until a consumer needs" it;
LLP 1011.001 D2 takes it for files. The crate's own test covers `requiredExtensions`
only (`host/svg-file/src/lib.rs:379`). Nothing in `issues/` or `QUEUE.md` names
`switch` or `systemLanguage`.
