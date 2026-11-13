# Windows Desk reads a whole file before applying its 1 MB cap

**Status:** Open
**Systems:** apps/windows-desk
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

Open calls `read_to_string` and only afterwards rejects a body longer than 1,000,000 bytes. A large file chosen in the dialog is allocated in full first. That can exhaust memory and abort the process instead of showing the error the over-limit arm already has. The dialog's filter includes all files, so a large text file is a normal choice. The edit control is already capped at that size.

## Where

`apps/windows-desk/src/native.rs`, the `OPEN` arm: `std::fs::read_to_string`, then `body.len() <= 1_000_000`.

## How to reproduce

Open a multi-gigabyte file from the File dialog. The process allocates the whole file before it can show "smaller than 1 MB".

## Direction

Reject from metadata, or read at most 1,000,001 bytes, before building a `String` of the whole file.
