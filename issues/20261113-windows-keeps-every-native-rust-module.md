# Windows keeps every native Rust module image in temp

**Status:** Open
**Systems:** logic
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

Loading a native Rust module writes `module.dll` under a unique `exact-rust-<pid>-<stamp>-<attempt>` directory in the temp folder. The file and directory are deleted only on Unix, including when `dlopen` fails. The path is not stored, and the retained-image table never removes it.

On Windows a failed load leaves the DLL on disk, and a successful load leaves it after the process exits. The file is locked only while the image is mapped. Each launch of an app with a native Rust module adds another copy, up to the 32 MiB image, under temp.

## Where

`logic/src/native.rs`, `map_symbols`. The delete is inside `#[cfg(unix)]`.

## How to reproduce

Load a native Rust data module on Windows, then exit. The `exact-rust-*` directory remains in the temp folder. A refused image leaves its directory too.

## Direction

Delete the file and directory when `Library::new` fails. Remove a mapped image when the process exits.
