# Safe areas in native routes

A route clears its controller's safe area. Use
`var(--exact-safe-area-inset-<edge>)` to read the route's inset, and
`env(safe-area-inset-<edge>)` for what the box's context leaves. A scroller's
padding and content read zero on the edges it insets.

Write `-exact-safe-area="top"` on a route to draw under its bar. A scroller
can reach past its border box with `-exact-safe-area-reach="top bottom"`.
Use `-exact-safe-area-extend` on an image or box to mirror its picture onto
selected edges.

See [Navigation stack](navigation-stack.md) and [Foldables](foldables.md).
