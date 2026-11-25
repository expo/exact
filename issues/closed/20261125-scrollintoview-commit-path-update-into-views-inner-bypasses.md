# scrollIntoView commit path (update_into_views_inner) bypasses admit_drag_files grant/URL validation and render_owned reconciliation

**Status:** Closed
**Resolution:** Bound session roles, ran admit_drag_files on all scrollIntoView commit phases, and reconciled transactional owned resources and tasks before and after seek phases in Runner::update_into_views.
**Systems:** runner (kernel commit pipeline, drag-out admission, owned reconciliation)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Back doors)

Verification: verified by reading.

At `9c0431fd5`, `Runner::update` (`runner/src/runner/render.rs:54-60`) branches to `update_into_views` -> `update_into_views_inner` (`runner/src/runner/into_view.rs:130-261`) whenever `self.into_view` is non-empty, bypassing both `Runner::render_owned` (`runner/src/runner/owned.rs:94-103` -> `runner/src/runner/owned_tasks.rs:72-96`) and `Runner::apply` (`runner/src/runner/apply.rs:48-111`). Direct calls to `Runner::scroll_into_view` and `Runner::scroll_into_view_at` (`runner/src/runner/into_view.rs:36-46, 70-93`) also enter `update_into_views_inner` directly.

Inside `update_into_views_inner` (`runner/src/runner/into_view.rs:142-261`), the runner commits `ops` (`begin_document_commit`, line 176), each `phase` (`continue_document_commit`, line 202), and `cleanup` (`continue_document_commit`, line 225) after calling `place_portals`, `prepare_live_times`, and `prepare_text_axes`, but never calls:
1. `self.admit_drag_files(...)` (`runner/src/runner/drag_files.rs:12-68`) on `ops`, `phase`, or `cleanup` (unlike `apply.rs:60`);
2. `self.bind_roles()` around the commit (unlike `apply.rs:52` and `document.rs:13`);
3. `self.reconcile_owned(&owners)` (which also runs `reconcile_sends`) and `self.reconcile_tasks(&owners)` when `self.transactional_render()` is true (unlike `owned_tasks.rs:72-96`).

### Trigger
1. **Drag admission bypass (action path):** A Contract action updates state bound to `draggable=true dataTransfer=DataTransfer(url=..., files=...)` on any element (e.g. a non-http(s) URL such as `file:///etc/passwd` or `javascript:...`, a non-`assets/` relative path such as `plans/app.exwf` or `Info.plist`, an ungranted `app:/` path, or on `Platform::Web` any `app:/` path) and invokes list-row `scrollIntoView("feed", key, ...)` in the same action (`runner/src/runner/commit.rs:800-811`).
2. **Drag admission bypass (row materialization path):** Calling `Runner::scroll_into_view` / `scroll_into_view_at` (or an action calling `scrollIntoView`) materializes virtualized list rows that carry `draggable=true dataTransfer=DataTransfer(...)` in `phase` (`into_view.rs:191-202`).
3. **Owned reconciliation bypass:** A Contract action updates state that mounts, unmounts, or changes props/gates/args of a child component owning a `resource`, `mutation`, or `task` (e.g. toggling an `if` arm outside the list) and calls list-row `scrollIntoView("feed", key, ...)` in the same action.

### Actual vs. Expected
- **Actual:** Unadmitted `DataTransferUrl` and `DataTransferFiles` values are committed verbatim into the kernel (`kernel/src/txn.rs` validates only prop kind), with no `"drag: refused: ..."` journal lines. Native hosts rely on runner-side admission: macOS (`host/apple/Sources/ExactDragOut/DragItemsMac.swift:17-43`) writes arbitrary `dataTransferUrl` strings straight onto `NSPasteboard` as `.URL` and exposes non-`app:/` file entries via `DragFiles.shipped` -> `AssetResolver.url` (`host/apple/Sources/ExactCore/Assets.swift:125-128, 174-187`), whose `validPath` does not enforce the `assets/` prefix that `exact_plan::is_shipped_drag_file` (`plan/src/lib.rs:991-997`) requires, allowing non-`assets/` bundle files (e.g. `plans/...`, `Info.plist`) to be dragged out; iOS (`host/apple/Sources/ExactDragOut/DragItems.swift:40-75`) registers non-http(s) `NSURL`s and resolves non-`assets/` bundle files the same way; painted/Android (`host/painted/src/presenter/drag_out.rs:208-239`, `host/android/canvas/src/main/java/dev/exact/canvas/DragOut.kt:124-138`) emits arbitrary `url` into `ClipData` and initiates `Request::Drag` for nodes whose file entries should have been stripped. (For `app:/` paths on Apple/Android, drop-time `shareOpen` / `Files.uri` still checks `fs.read` grants, though bogus drag promises are initiated.) In addition, mounted/unmounted child components with owned resources, sends, or tasks are never reconciled on a `scrollIntoView` commit: newly mounted owned resources stay stuck on placeholders and owned tasks stay unarmed, while unmounted owned resources/sends are not let go and unmounted owned tasks stay armed until their timer fires and traps with `BadScope` (`runner/src/runner/owned_tasks.rs:213-229`). On multi-session single-threaded hosts (LLP 1121 stage 5), omitting `bind_roles()` lets kernel commits resolve role overrides against another session's thread-local table.
- **Expected:** Per LLP 1094.000 D2 (`runner/src/runner/drag_files.rs:1-6`, `runner/src/runner/document.rs:20`), every commit path must run `admit_drag_files` before ops enter the kernel so hosts never classify or carry unadmitted drag data. Per LLP 1035.005.004 (`runner/src/runner/owned.rs:6-17`, `owned_tasks.rs:70-96`) and LLP 1121 stage 5 (`apply.rs:52`), every settled render must bind the runner's roles and reconcile owned child resources, sends, and tasks.

### Verification & Siblings Checked
Verified by reading the exact commit paths in `runner/src/runner/` and host drag presenters at `9c0431fd5`. Siblings checked:
- `runner/src/runner/apply.rs:48-111`: calls `bind_roles()` (line 52) and `admit_drag_files(&mut ops)` (line 60); its `cleanup` tail (`apply.rs:96-103`) comes from `reconcile_reorder` (`end_preview`/`set_outgoing`/`end_session`), which only emits preview/style/hide ops.
- `runner/src/runner/into_view.rs:12-31` (`reveal_collection_index`): delegates to `self.update_tree(false, ...)`, so it goes through `render_owned` and `apply` (not affected).
- `runner/src/runner/document.rs:12-38`: calls `bind_roles()` and `admitted_drag` (not affected).
- `runner/src/runner.rs:1135-1137` (`boot_inner`): calls `runner.apply(ops)` and `runner.boot_owned(&mut receipt)` (not affected).
- `host/web-js/drag.js:39-51`: admits on DOM prop hooks directly, independent of `scrollIntoView` (not affected).

### Suggested Fix & Regression Test
In `runner/src/runner/into_view.rs`:
1. Bind `let _roles = self.bind_roles();` at the start of `update_into_views_inner`.
2. Call `self.admit_drag_files(&mut ops)`, `self.admit_drag_files(&mut phase)`, and `self.admit_drag_files(&mut cleanup)` before `begin_document_commit` / `continue_document_commit`.
3. When `settled && self.transactional_render()`, run the `render_task_owned` reconciliation loop (`reconcile_owned` + `reconcile_tasks` and follow-up `update_ops` passes, keeping rollback behavior on refusal before starting the kernel commit) before entering the phased `begin_document_commit` / `into_view` loop (or reconcile owned state on the pre-seek `ops` phase before `begin_document_commit`).

Add regression tests in `contract/cli/tests/it/drag_out.rs` and `contract/cli/tests/it/collection_into_view.rs`:
- An action that sets invalid `dataTransfer` `url` and `files` alongside `scrollIntoView("feed", key)` (and a `scroll_into_view_at` materializing a virtualized row with invalid `dataTransfer`) asserts that invalid URLs/files are cleared from kernel props and journaled as `"drag: refused:"`.
- An action that toggles an `if` mounting/unmounting a child component with an owned `resource` and `task` alongside `scrollIntoView("feed", key)` asserts that the resource settles (or is let go) and the task is armed (or disarmed).

### Siblings checked

Checked apply.rs (calls bind_roles and admit_drag_files on ops; cleanup from reconcile_reorder emits only reorder preview/hide ops), reveal_collection_index (goes through update_tree -> render_owned -> apply), document.rs (calls bind_roles and admitted_drag), boot_inner (calls apply and boot_owned), and host/web-js/drag.js (admits in DOM prop hooks). None of the siblings share the bug.
