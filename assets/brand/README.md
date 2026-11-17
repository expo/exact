# Exact brand assets

[`exact-mark.svg`](./exact-mark.svg) is the canonical designed mark for Exact. Its 161.803:100 outer geometry is a golden rectangle; the divider at x=100 partitions it into a square and a reciprocal golden rectangle.

The SVG uses a portable mid-neutral stroke that remains legible on light and dark surfaces. Consumers may tint the monochrome mark for their context, as the docs site does. Preserve its view box, geometry, rounded stroke treatment, and accessible title when deriving other formats.

The install page (`scripts/install-page.mjs`) inlines this file in its footer, and the docs site uses it as its favicon (`docs/site/tools/postbuild.ts`) and in `docs/site/fixtures/components.md`. The docs site's home link draws an adaptation, `docs/site/assets/icons/exact-mark.svg`, with the stroke drawn as fills so the icon can be tinted; it should remain geometrically identical to this asset.
