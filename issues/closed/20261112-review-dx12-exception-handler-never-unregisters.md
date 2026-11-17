# D3D12 device teardown does not unregister its exception handler

**Status:** Closed
**Resolution:** The D3D12 debug-message handler's registration keeps the handle AddVectoredExceptionHandler returned and the last device passes it to RemoveVectoredExceptionHandler; a device registers only once its construction can no longer fail.
**Systems:** vendor/wgpu-hal, Windows GPU backend
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `vendor/wgpu-hal/src/auxil/dxgi/exception.rs:14-27`
discards the handle returned by `AddVectoredExceptionHandler`. When the
last device is dropped, it passes `output_debug_string_handler`'s function
address to `RemoveVectoredExceptionHandler` instead. Removal requires the
registration handle, so the handler remains registered while the Rust
reference count returns to zero.

`vendor/wgpu-hal/src/dx12/device.rs:55-60` registers this handler for a
device with validation enabled, and `dx12/mod.rs:790-797` unregisters it
on drop. Exact's GPU dependency enables the D3D12 backend. Repeated device
creation and teardown therefore accumulate process-wide callbacks and
make subsequent exception dispatch walk the retained registrations.

A bounded probe compiled the unchanged registration/counting functions
against an adapter implementing the documented handle contract. After
three register/unregister cycles, the Rust count was zero after each
cycle, while the adapter retained one, two, then three registrations.
The probe is `/tmp/exact-review-20261112/dx12-handler-lifetime.rs`.
This verifies the arguments and count transitions; a native Windows run
was not performed.

Store the successful registration handle with the count and pass that
handle on the final removal. Handle registration failure explicitly, and
roll registration back if device construction fails before it acquires
its teardown owner. Validate repeated device creation/drop and failed
construction without leaving a process-wide handler behind.

Microsoft documents the returned handle in
[AddVectoredExceptionHandler](https://learn.microsoft.com/en-us/windows/win32/api/errhandlingapi/nf-errhandlingapi-addvectoredexceptionhandler)
and requires it as the argument to
[RemoveVectoredExceptionHandler](https://learn.microsoft.com/en-us/windows/win32/api/errhandlingapi/nf-errhandlingapi-removevectoredexceptionhandler).

Checked open and closed filesystem issues, `QUEUE.md`, and all 275 cached
GitHub issues; no existing report covers this handler lifetime.
