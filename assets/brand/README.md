# Exact brand assets

The designs are in the Figma file [Exact](https://www.figma.com/design/P0bENr1Brpx4hivBqpbkMF/Exact?node-id=22-1146), frame "Exact branding".

[`exact-mark.svg`](./exact-mark.svg) is the canonical mark for Exact: a rectangle split into a large panel on the left and a column on the right that is divided again into smaller panels. Its 420:246 view box and line positions are the Figma lockup's. Figma draws the lines 3 units wide, for display sizes; this file draws them 16 units wide, so that they stay about 1 px wide at the 16 to 20 px heights where Exact shows the mark. The stroke is a portable mid-neutral that remains legible on light and dark surfaces, with square ends and corners. Consumers may tint the monochrome mark for their context, as the docs site does. Preserve its view box, geometry, line weight and accessible title when deriving other formats.

[`exact-avatar.svg`](./exact-avatar.svg) is the Figma avatar, unchanged: a square mark split into halves and quarters, with "Ex", on a dark square. It is for square places at 180 px and larger, such as an app or account icon.

[`exact-wordmark.svg`](./exact-wordmark.svg) is the Figma wordmark, unchanged: "Exact" on a dark 807:400 panel.

Readers:

- The install page (`scripts/install-page.mjs`) inlines `exact-mark.svg` in its footer.
- The docs site's home link draws `docs/site/assets/icons/exact-mark.svg`, the same geometry with the lines drawn as fills so that the icon can be tinted. Keep it identical to `exact-mark.svg`.
- The docs site's favicons are in `docs/site/assets/favicons`. `apple-touch-icon.png` is `exact-avatar.svg` at 180 px. At 16 to 48 px the avatar's lines would be thinner than a pixel and "Ex" could not be read, so `favicon.svg` draws the avatar's square mark without "Ex", at three quarters of the tile and with lines 2.5 units wide on a 32-unit tile; the PNGs are that file at 16, 32 and 48 px, and `favicon.ico` holds the 16 and 32 px PNGs.
- The docs site's share image, `docs/site/assets/og-image.png`, is `exact-wordmark.svg` at 1200 px wide. Every page names it as `og:image` and `twitter:image` through its `head`.
- `docs/site/fixtures/components.md` shows `exact-mark.svg` as its sample image.
