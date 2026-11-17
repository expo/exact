# Removing the experiment

Delete these files, then the directory:

- `scripts/see/.gitignore`
- `scripts/see/README.md`
- `scripts/see/REMOVING.md`
- `scripts/see/Mac.swift`
- `scripts/see/checkpoint.mjs`
- `scripts/see/compare.mjs`
- `scripts/see/main.mjs`
- `scripts/see/platform.mjs`
- `scripts/see/see.test.mjs`

Delete the four-line hook in `scripts/exact.mjs` beginning with
`// see: removable (LLP 1172.021)` and ending at that `if`'s closing brace.
There are no other hooks, dependencies, core changes, rules or test registrations.
`see.test.mjs` checks the file list and scans tracked source for outside references.

Delete `scripts/see/.cache/` if it exists (on-demand Mac binaries). Run evidence
lives only in the directories explicitly passed to `--out`; delete those too.
The existing iOS touch runner and its cache belong to the driver and stay.
No simulator is created by the command. Any simulator created for a manual
qualification run is named in the lane report and removed by that run's owner.

With the hook absent, the original command dispatch resumes. With the hook
present but `see` unrequested, no experiment module is loaded.
