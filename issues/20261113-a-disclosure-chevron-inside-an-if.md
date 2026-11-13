# A disclosure chevron inside an if leaves the row link-coloured

**Status:** Open
**Systems:** contract lower, grouped rows
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13
**Related:** issues/20261110-compiler-walks-views-and-expressions-by-hand.md

## What is wrong

A link that ends in a disclosure chevron is recoloured to the label, because that row pushes rather than navigating as a link. The test looks at the last shown child and asks whether that node itself is the chevron symbol. A last child that is `if` … the chevron does not match, so the link never receives `color` and stays `LinkText`.

The chevron's own ink is still the tertiary disclosure colour, because the walk does enter `if`. An optional disclosure (`if canOpen` then the symbol) draws link-coloured text beside a grey chevron. The comment on the function says a link that ends in a disclosure is the label's colour on every host.

## Where

`contract/lower/src/grouped/actions.rs` `disclosure`: the `tag == "link"` arm calls `chevron` on the last shown child. `chevron` matches a disclosure symbol, not an `if` whose shown arm is that symbol.

## How to reproduce

A grouped `link` whose last child is `if canOpen` containing `image "symbol:chevron.right"`. The title is link-coloured. The same chevron as a direct child is the label colour.

## Direction

Treat the last shown child as a disclosure when it is the chevron, or an `if` whose shown arms are that one symbol, the same way the leading-symbol check reads arms.
