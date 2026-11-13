# Web JS test fixtures fail the navigation-root rule, so four suites fail on main

**Status:** Closed
**Resolution:** Each fixture's navigation root is its view's root: chrome sits in the root or a route, the cleared-styles cases build one app each, and the status cases build one app per row from only that row's branches; the three cases that needed a nested or second root are removed.
**Systems:** host/web-js tests (document-agent.test.mjs, cleared-styles.test.mjs, sheet-leave.test.mjs, document-render.test.mjs), host/web-js/conformance (heads, heads-split, navroot)
**Severity:** P2
**Author:** Claude
**Date:** 2026-11-12

## What is wrong

Since `8db3f857` the compiler refuses a navigation root (`navigationKey` with
`navigationBack`) anywhere but the app view's root element (`lower-route-place`).
Fixtures written before it put the root inside a `column` beside buttons, in an
`if` arm, or several roots in one app, so they no longer compile:

- `document-agent.test.mjs`: the status-document fixture shared by its
  nineteen `test.each` rows, and "a second navigation root keeps its split list
  live", which also reads `conformance/heads-split.contract`.
- `cleared-styles.test.mjs`: "an empty bound background clears routes over
  authored classes and the neutral sheet" (fourteen roots in one app).
- `sheet-leave.test.mjs`: all three tests.
- `document-render.test.mjs`: "exclude a covered route's deeper head" and "mark
  the bar".
- `conformance/heads.contract`, `heads-split.contract` and `navroot.contract`,
  which the conformance run builds.

No browser test then covers the JS target's sheet path, and the status-wait
cases of the server renderer run nowhere.

## Direction

Rewrite each fixture so its root is the view's root and its scenario is
unchanged: chrome becomes the root's non-route children or a route's own
content, one case per app where a test needed several roots, one app per test
row where one fixture mixed stacks and tabs. A scenario that needs two roots, or
a root inside a route, cannot be written any more; say so and drop it.

## Found (2026-11-12)

The status rows needed more than a moved root. A root that holds tabs keeps its
routes in its panels, so stacks and tabs cannot share one app, and the JS
target's status analysis (`host/web-js/src/answers.rs`) fixes a root's selection
only from a literal key and routes no `if` arm hides: wrapped in the shared
`if top(nav).name == …` chain, the "unselectable" cases waited for data. Each
row now builds its own app, and a row of one branch writes it straight under the
root. The first row is split into pages, routes and tabs.

Removed, as two roots or a root inside a route are now refused:

- "a second navigation root keeps its split list live". Its covered copy of a
  control cannot come earlier in one root: a route before the list would be the
  stack's first, which the split takes as its list.
- `inactiveSelector`: a nested root whose key reads data, in an inactive route.
- `idInactiveRows`: id shadows inside a nested root's covered route. Moved to a
  covered route of the one root, the shadows no longer count (200, not 404), so
  it would test another rule.

`conditionalInactive`, `conditionalInactiveTab`, `inactiveData` and `idRows`
lost the nested or sibling roots they held as distractors (a nested root's
tablist became a tablist inside the route); each still asserts its outcome.
The seven cases in QUEUE.md's "Seven web-js document-agent cases" fail with or
without this change: their app has no root.
