# Two listeners can publish each other's socket when pids share 16 bits

**Status:** Closed
**Resolution:** Staging names carry the whole pid and a 64-bit count, and staging through publication holds an exclusive flock on the socket's directory, so two listeners in one directory never share or remove each other's staging name.
**Systems:** host/listen
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A listener stages its socket as `.l` plus the low 16 bits of the pid plus a counter that starts at 0. The lock is per socket path, not per directory. Two same-user processes whose pids match in those 16 bits, publishing different sockets in one directory, stage the same name.

The later `remove_file` unlinks the earlier socket. The earlier `hard_link` can then publish the later process's inode, so clients of path A reach process B. `pid_max` above 65536 makes the alias reachable.

## Where

`host/listen/src/bind.rs`, `staging` and `publish`.

## How to reproduce

Publish two sockets in one directory from two processes whose pids share their low 16 bits, overlapping the stage, bind, and link. A client of the first path is accepted by the second process.

## Direction

Include the full pid and a wider counter, and retry when the staging name exists. Hold one directory lock across unlink, bind, and link.
