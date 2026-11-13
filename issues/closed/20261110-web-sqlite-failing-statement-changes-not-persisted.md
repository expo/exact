# Browser SQLite does not save the rows a failing `OR FAIL` statement kept

**Status:** Closed
**Resolution:** An `execute` that throws after changing rows outside a transaction (SQLite's FAIL conflict resolution) now saves the database file before its error reaches the app.
**Systems:** host/web/storage-worker.js
**Severity:** P3
**Author:** Claude (Opus 5.5), from GPT-6 Astra's whole-project review (2026-11-10)
**Date:** 2026-11-10

Line references are at `46f3d608`.

## What is wrong

The browser's SQLite database lives in memory and is exported to its file after each
`execute` (`host/web/storage-worker.js:225-226`). When the statement throws, `run`
(`:225`) leaves `dispatch` before `persist` (`:140-160`) is reached.

A statement can throw after changing the database for good. With the `FAIL` conflict
resolution (`INSERT OR FAIL`, `UPDATE OR FAIL`) or a trigger's `RAISE(FAIL, …)`, SQLite
keeps the changes the statement made before the error, and outside a transaction they
are committed (<https://www.sqlite.org/lang_conflict.html>). The connection then answers
queries with those rows while the file does not hold them. They are saved only if a
later `execute` succeeds before the database is closed; otherwise reopening loses them.
A native host's SQLite writes them to its file as it commits.

`transaction` is not affected: it rolls back on any error (`:214-215`), so the file and
memory agree.

## To see it

Confirmed by reading the worker; SQLite's behaviour checked with `bun:sqlite`: after
`CREATE TABLE t (x INTEGER UNIQUE); INSERT INTO t VALUES (1)`, the statement
`INSERT OR FAIL INTO t VALUES (2),(1)` throws `UNIQUE constraint failed` and the table
holds 1 and 2, with no transaction open. On the web, close and reopen the database after
that statement: it holds only 1.

## What a fix is

In `dispatch`, when a non-query statement throws and the connection is in autocommit
(`sqlite3_get_autocommit`) with `sqlite3_total_changes` moved, persist before rethrowing
the statement's error; if that persist fails, `persist` already closes the connection.
A test in the storage worker's suite: the statement above, then close, reopen and count.
