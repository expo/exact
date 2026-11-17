# A test's `request` step refuses a scalar JSON body that the grammar and its own message admit

**Status:** Closed
**Resolution:** `request_step` takes a body when the next token begins any JSON value (a string, a number, `-`, `true`, `false` or `null` as well as `{` or `[`), as the grammar and `json_value`'s message say.
**Systems:** contract/syntax (parser/steps.rs), docs/contract-grammar.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`docs/contract-grammar.md` ("Authored tests", lines 704 and 726-728) gives the
step as

```
"request" METHOD STRING [ json ] [ "as" ( IDENT | STRING ) ] [ "expect" NUMBER ] NL
json = "{" … "}" | "[" … "]" | STRING | [ "-" ] NUMBER | "true" | "false" | "null" ;
```

`json_value` parses all of those and says so in its refusal
(`contract/syntax/src/parser/steps.rs:486-523`: "a request's body is JSON (an
object, a list, a string, a number, true, false or null)"). `request_step` calls it
only when the next token opens an object or a list (`steps.rs:350-353`):

```rust
let body = match self.peek_kind() {
    TokenKind::Punct("{") | TokenKind::Punct("[") => Some(self.json_value()?),
    _ => None,
};
```

So a scalar is accepted inside a container and refused as the whole body, by the
end-of-line check, which does not say what a body may be.

## The failure

```
test "r"
  request PATCH "/x" "str"
```

`contract test` prints:

```
app.test.contract:2:22 [syntax-expected-newline] expected end of line, found a string
```

and the same for `5` ("found number 5"), `-5` ("found `-`"), `true` and `null` in
that place. Expected: the body `"\"str\""` (`5`, `-5`, `true`, `null`), as
`{"a": 1}` and `[1, "a", null]` give `"body":"{\"a\":1}"` and
`"body":"[1,\"a\",null]"` today.

## Verification

Ran `contract test` (debug build) on the step with each of the seven bodies. The
five scalars were refused as quoted, the object and the list printed the bodies
quoted, and `request PATCH "/x" {"a": none}` printed the `json_value` message that
lists the scalars. Not run: a drive that sends a body.

## Direction

Take a body when the next token is a string, a number, `-`, `true`, `false` or
`null` as well; `as` and `expect` are the only other words that may follow the
path, so nothing becomes ambiguous. Or narrow the grammar's `[ json ]` and the
message to an object or a list.

## Related

None found in `QUEUE.md`, `issues/` or `docs/agent-pitfalls.md`.
