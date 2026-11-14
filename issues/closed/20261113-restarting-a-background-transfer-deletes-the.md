# Restarting a background transfer deletes the saved record before the new one is valid

**Status:** Closed
**Resolution:** A restart checks every request, then copies the bodies and writes the new `record.json` into a staging folder that replaces the previous registration's folder only once all of it is there, so any refusal leaves the previous record and its files as they were.
**Systems:** host/apple
**Severity:** P1
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

Restarting a finished background transfer deletes that transfer's folder before the new requests are known to be valid. `start` removes `folder(id)` and creates it again, then returns `TypeError` for a URL that is not `http` or `https`, and `NotAllowedError` when a `bodyFrom` file cannot be copied. `keep`, which writes `record.json` and replaces the in-memory record, runs only after every request has a task.

The previous bodies are already gone. The in-memory record still describes them, so the app can show a transfer whose files are missing. The next launch reads only `record.json`, which was in the deleted folder, so the transfer is gone.

## Where

`host/apple/Sources/ExactKit/Transfers.swift`, `start` and `keep`. `install` loads a record only from `record.json`.

## How to reproduce

Finish a background transfer so its folder holds `record.json` and a downloaded body. Start the same id again with a URL whose scheme is not `http` or `https`, or with a `bodyFrom` path that cannot be copied. The call returns an error name. The previous folder is gone.

## Direction

Check every request, and copy every `bodyFrom`, before removing the previous folder. Replace that folder only once the new record can be kept. On any error, leave the previous record and its files as they were.
