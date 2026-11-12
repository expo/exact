# The testing guide publishes a private machine's tailnet name and SSH login

**Status:** Closed
**Resolution:** "Windows: the NucBox" now writes `<host>` and `<user>` where it named the machine's tailnet host and the SSH login (the private LLP 1115.004 names the host; no other tracked file carried either), and git history still holds the old text, which was not rewritten.
**Systems:** docs/testing.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

`docs/testing.md`, section "Windows: the NucBox" (lines 297-313 at `dd6f5dcd`), gives the tailnet host name of the Windows test machine, the account name to log in as, the exact `ssh` command, and the machine's hardware and display. It was written on 2026-10-23, before the repository became public on 2026-11-02.

A reader outside the tailnet cannot reach the machine, and they cannot use the section either: it tells them how to run Windows checks on a machine only its owner can log in to. AGENTS.md's rule for diaries ("secrets and personal data may not" appear) reads as the intent for the rest of the public tree.

## Direction

Keep what a contributor can use (the toolchain versions the Windows oracle was recorded with, that an SSH session is not the desktop, how to send commits as a bundle) and move the host name, the login and the `ssh` line to a place only the team reads, such as the private LLP corpus beside LLP 1115.004 D13, which the section already cites.

## Verification

Read the section. Nothing was connected to.
