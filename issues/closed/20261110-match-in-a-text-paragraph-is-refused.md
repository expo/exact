# A `match` inside a text paragraph is refused though its arms hold only runs; the same paragraph with `if` compiles

**Status:** Closed
**Resolution:** A `match` among a paragraph's runs is admitted as `if` and `for` are (LLP 1011.002 D1), and an image under one in a multi-column paragraph is refused as under an `if`.
**Systems:** contract/lower/src/nodes.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, a `text` that holds runs checks that each child is a run
(`contract/lower/src/nodes.rs:837-848`, lines joined here):

```rust
fn run(c: &Node, images: bool) -> bool {
    match c {
        Node::Element { tag, .. } => matches!(tag.as_str(), "text" | "time") || images && tag == "image",
        Node::Each { body, .. } => body.iter().all(|c| run(c, images)),
        Node::When { then, otherwise, .. } => then.iter().chain(otherwise).all(|c| run(c, images)),
        _ => false,
    }
}
```

It reads through `for` and `if` and answers false for `Node::Match`, so a `match` among a
paragraph's runs is refused whatever its arms hold. `has_image` just below (`nodes.rs:856-865`), which
refuses an image in a multi-column paragraph, has the same three arms.

## The failure

```contract
component App
  state name = some("Ada")
  action clear
    name = none
  view
    column
      text testId="greeting"
        text "Hello, "
        match name
          case some(n)
            text n
          case none
            text "stranger"
      button "Clear" press=clear testId="clear"
```

Observed, `contract build`:

```
match.contract:9:9 [lower-leaf-children] a text paragraph holds `text` or `time` runs and `image`s
 9 |         match name
```

Expected: it compiles. Every child the `match` can show is a `text` run, which is what the message
asks for. The same paragraph with `state known = true` and `if known` … `else` compiles, and no guide
says a paragraph takes `if` and `for` but not `match`.

## Verification

Ran the source above and its `if` twin through `contract build` and through a test that boots the
plan and reads `agent::tree` before and after `clear`
(a temporary test under `contract/cli/tests/`): the `match` test fails with the refusal, the
`if` test passes. With one arm added to `run`,
`Node::Match { some, none, .. } => some.1.iter().chain(none).all(|c| run(c, images))`, both pass and
the two trees are the same: runs `"Hello, "` and `"Ada"`, then `"Hello, "` and `"stranger"`. So
nothing after this check needs the refusal.

## Direction

Add the `Match` arm to `run` and to `has_image`.

## Related

`issues/20261110-class-native-grouped-button-match-leaks-ua-row.md` is another walk that reads `if`
and not `match` (`grouped::unsheet`). Neither names this one. The grouped list's `text_lines`
(`contract/lower/src/grouped/cell.rs:69`), `line` (`grouped/lines.rs:98`) and `fold`
(`grouped/messages.rs:114`) also read `if` only; I did not test what a `match` does there.
