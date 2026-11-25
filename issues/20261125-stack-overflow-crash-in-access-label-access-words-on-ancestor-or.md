# Stack overflow crash in `access_label`/`access_words` on ancestor or cyclic `aria-labelledby`

**Status:** Open
**Systems:** host/painted (Android TalkBack, Windows UIA, Linux AT-SPI, top-layer menu projection)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Quadratic work, stack depth and allocation)

Verification: verified by reading.

At `9c0431fd5`, `Presenter::access_label` (`host/painted/src/presenter/access.rs:408-427`) resolves `PropId::AccessibilityLabelledBy` IDs and, when a referenced target has no non-empty `aria-label`, calls `self.access_words(&n, true)` (`access.rs:417`). `Presenter::access_words` (`access.rs:468-497`) iterates the target's visible children matching `all || !self.access_pressable(n)` (`access.rs:488`) and unconditionally calls `self.access_label(&n)` (`access.rs:490`) with neither a visited set nor a non-recursive mode. `Presenter::access_hint` (`access.rs:432-453`) similarly follows `PropId::AccessibilityDescribedBy` into `self.access_name(&n)` (`access.rs:441`), which calls `self.access_label` (`access.rs:560`) and `self.access_words` (`access.rs:566`).

### Reachable trigger
Neither the Contract compiler (`contract/lower/src/tags.rs:801-804`) nor the kernel (`kernel/tables/schema.json:1560,1578`) restricts `aria-labelledby` or `aria-describedby` from targeting an ancestor or forming a cycle. Author a standard ARIA labelled-row pattern where a child control references its enclosing container (with no `aria-label` on the container):

```contract
row id="wifi-row"
  text "Wi-Fi"
  input type="checkbox" role="switch" checked=on change=flip aria-labelledby="wifi-row"
```

or any mutual cycle between two containers whose descendants reference each other (`id="a"` containing a child with `aria-labelledby="b"`, and `id="b"` containing a child with `aria-labelledby="a"`). Any of the following entry points then triggers unbounded mutual recursion:
- Android TalkBack (`host/painted/src/canvas/access.rs:23` -> `Presenter::reader_access` at `access.rs:1216` -> `access_tree` at `access.rs:970` -> `access_walk_contents` at `host/painted/src/presenter/access/walk.rs:191-192,234`).
- Windows UIA / Linux AT-SPI (`host/painted/src/presenter/accesskit.rs:69`, `host/windows/src/accessibility/model.rs:154,575` -> `Presenter::reader_access`).
- Top-layer menu projection even with assistive technology off (`host/painted/src/presenter/top/shape.rs:280,288` -> `self.access_label`; `host/painted/src/presenter/top/status.rs:201`; `host/painted/src/presenter/access/surface.rs:217`).

### Actual vs. expected
- **Actual:** `access_label(&switch)` resolves `wifi-row` and calls `access_words(&row, true)`. Because `all == true` (and `!access_pressable(&switch)` is also true), `access_words` visits `switch` and calls `access_label(&switch)` at `access.rs:490` before any control-type check, recursing infinitely (`access_label` -> `access_words` -> `access_label`) until the process aborts with a stack overflow.
- **Expected:** Per W3C Accessible Name and Description Computation (AccName) 1.2 §4.3.1 step 2B and Apple's sibling implementation (`host/apple/Sources/ExactKit/Accessibility.swift:107-128`), `aria-labelledby` traversal must not recursively follow `aria-labelledby` on referenced targets or their descendants, and cycle checks must guarantee termination, returning `"Wi-Fi"`.

### Verification and siblings checked
Verified by reading the full call chain from `reader_access` / `access_tree` / `top/shape.rs` through `access.rs:408-497` and `access/walk.rs`. Siblings checked:
- `host/apple/Sources/ExactKit/Accessibility.swift:86-96,106-130,196-203`: not affected (`referencedLabel` calls `contentText(naming: false, ...)`, skipping `child.authoredLabel` at `L89` so `aria-labelledby` never recurses; `accessibleDescription` calls `accessibleName` and never re-enters `accessibleDescription`).
- `host/apple/Sources/ExactKit/Mac/ToolbarReadsMac.swift:197-219`: not affected (cycle-guarded with `walked` and `r.names` sets).
- `host/apple/Sources/ExactKit/MenuShape.swift:51`: not affected (delegates to `authoredLabel`).
- `host/painted/src/presenter/top/shape.rs:478-496` (`section_title`): not affected (calls plain `text_of`, never `access_label`).
- `host/web/src/element.rs:1048-1049`: not affected (delegates to browser DOM).
- `host/terminal/`: does not resolve `AccessibilityLabelledBy` or `AccessibilityDescribedBy`.

### Suggested fix and regression test
Split the recursive helper in `host/painted/src/presenter/access.rs` so `access_words` takes a `follow_labelledby: bool` flag (set `false` when called from `access_label`, matching `Accessibility.swift`'s `naming: false` — reading only a child's own non-empty `aria-label` when `false` instead of calling `access_label`) and carries a visited `BTreeSet<ViewId>` (or `NodeKey`) to prevent any cycle across `aria-labelledby` / `aria-describedby`. Add unit tests in `host/painted/src/presenter/access_tests.rs` covering (1) a child switch/button with `aria-labelledby` naming its enclosing container, (2) a mutual two-container `aria-labelledby` cycle, and (3) `aria-describedby` pointing to a container with an ancestor-referencing child, asserting `access_tree()` terminates with the expected non-cyclic name and hint.

### Siblings checked

Checked host/apple/Sources/ExactKit/Accessibility.swift:86-130,196-203 (safe: naming=false), host/apple/Sources/ExactKit/Mac/ToolbarReadsMac.swift:197-219 (safe: walked set), host/apple/Sources/ExactKit/MenuShape.swift:51 (safe), host/painted/src/presenter/top/shape.rs:478-496 (safe: text_of), host/web/src/element.rs:1048-1049 (browser DOM), and host/terminal/ (not resolved).
