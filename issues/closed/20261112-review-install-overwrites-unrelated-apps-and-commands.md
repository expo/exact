# Installing an app overwrites unrelated bundles and command targets

**Status:** Closed
**Resolution:** `install` and `uninstall` replace or remove a bundle only when it carries the app's bundle identifier and a command only when it is the file `install` wrote, refusing links and anything else unchanged; a new bundle is staged and checked before it is renamed into place, and the shim is renamed over its path.
**Systems:** scripts/exact.mjs, macOS installation
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `install` derives its destination from the app's display name,
deletes any existing bundle there, and writes its command shim without
checking who owns either path (`scripts/exact.mjs:217-245`). A different app
with the same name is removed. An existing command is overwritten; if it is
a symlink, `writeFileSync` follows it and replaces the target's contents.
The subsequent `chmodSync` follows it too.

App names and command names can collide in ordinary use. Matching the new
build's identity does not establish ownership of the installation it is
about to replace. `uninstall` has the same missing ownership check: it
removes both derived paths even if another app or command has since taken
them.

**Verification.** `/tmp/exact-review-20261112/install-ownership-probe.mjs`
executes the unchanged `install` function in a private temporary directory.
The existing bundle holds an unrelated marker and the command path is a
symlink to a separate existing file. The marker is deleted and the symlink
target becomes the new shim. Building, copying and Launch Services are
controlled substitutes; the removal, write and chmod use the real filesystem.
No installed application, real command, or GUI was touched. Bun 1.4.2 was used.

Before replacing or removing an installation, establish that both paths
belong to this app. Refuse unrelated destinations with a clear collision
message. Replace a verified shim atomically without following a symlink;
stage and verify a bundle before replacing the old one, so a failed copy
does not destroy a working installation.

Check installation and uninstallation with an unrelated bundle, regular
command, symlink and matching prior Exact installation. A failed install
must preserve the prior usable files. No matching filesystem or GitHub
issue was found in this review's snapshot.
