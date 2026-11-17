# `forma import css` prints no kind for a token whose name spells a role and does not say it overrides that role

**Status:** Closed
**Resolution:** CSS imports report each token's kind and name any platform role it overrides.
**Systems:** contract/cli/src/forma_import_css.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

`contract forma import css` writes each `:root` custom property `--name` as the token `name` (LLP 1153 D6.2), and a token whose name spells a colour role overrides that role (LLP 1121 D3). So a stylesheet variable named `background`, `label`, `field`, `separator`, `fill`, `placeholder`, `highlight` or `canvas` becomes the app's override of a platform role. That follows from the two rules and is not the defect.

The defect is the import's report. It prints each token it wrote with its kind. For a token that overrides a role it prints no kind, and nothing says the token is an override. The report looks each kind up by the name it wrote (`contract/cli/src/forma_import_css.rs:211-218`, at `dd6f5dcd`). The checker names an override by its role's spelling (`-exact-background`, `Field`), so the lookup finds nothing and prints an empty string.

## The failure

`site.css`, imported into an app with no design:

```css
:root {
  --ink: #111111;
  --background: #ffffff;
  --label: #333333;
  --field: #f2f2f2;
}
@media (prefers-color-scheme: dark) {
  :root { --ink: #eeeeee; --background: #000000; }
}
```

`contract forma import css site.css --app .` exits 0 and prints

```
  ink = light-dark(#111111, #eeeeee)  color
  background = light-dark(#ffffff, #000000)  
  label = #333333  scheme=fixed  
  field = #f2f2f2  scheme=fixed  
```

`contract forma ir .` then lists `-exact-background` with `"role": "background"`, `-exact-label` with `"role": "label", "scheme": "fixed"`, and `Field` with `"role": "Field", "scheme": "fixed"`. The output gives the author no sign that the import set the platform's label colour to `#333333` in both schemes.

Expected: each line carries its kind (`color`), and a token that overrides a role says which. `forma import <kind>` offers each role override apart for that reason (LLP 1121 D14: "A design stays the platform's unless the person chooses otherwise").

## Verification

Ran the built `contract forma import css` on the stylesheet above against a one-component app, then `contract forma ir`. The outputs are as quoted. No host was run on the result.

## Direction

Take each printed kind from the folded token, matched by its declared name or by its role, and name the role on the line of a token that overrides one.

## Related

`scripts/forma-import-css.test.mjs` expects `--field: Canvas` to be written as `field = Canvas`, and `--link` to be refused by the compiler (`lower-theme-tint`), so writing the override is intended. Neither test reads an override's report line. LLP 1121 §10 (stage 1) names a warning for a probably unintended override as a candidate diagnostic of the language; this issue is only the import's report.
