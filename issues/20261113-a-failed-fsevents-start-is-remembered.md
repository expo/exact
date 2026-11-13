# A failed FSEvents start is remembered as installed

**Status:** Open
**Systems:** host/apple
**Severity:** P3
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`follow` replaces the stream, and if `Stream::start` returns none it still assigns `self.folders`. `FSEventStreamStart` can fail, and that path releases the stream. The next sync with the same folders returns before starting, so those watches never announce. The folder cache has no failed state. File events stay dropped until the watched folder set changes.

## Where

`host/apple/src/doc_watch.rs`, `follow` and `Stream::start`.

## How to reproduce

Fail `FSEventStreamStart` for a document watch (the stream is released and `start` returns none). Change a file in that folder. The watch does not fire, and a later sync of the same folders does not retry.

## Direction

Leave `folders` unchanged when start returns none, and retry `Stream::start` on the next sync while the stream is still absent.
