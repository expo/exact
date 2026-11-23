```prompt Fix a port that is taken
Make the Exact app in this folder serve, test and drive on the web when a port is taken, or when Bun or Chrome is not found. Use https://github.com/expo/exact/blob/main/docs/recipes/ports-and-browsers.md as the source for each step. The Exact checkout has the same page at docs/recipes/ports-and-browsers.md.

1. If this folder is not an Exact app (it has no exact.mjs and app.contract), stop and tell me.
2. If I did not say which command failed and what it printed, ask me, or run it and read its message. If I want a fixed port, ask me for its number.
3. A held port is refused. A second web loop of the same target for this app refuses and names the first loop's URL and PID: use that URL, or ask me before you stop that loop. Do not stop a process that you did not start.
4. To choose the port, add `--port <n>` to `web`, `test web` or `agent web`, or set `EXACT_PORT=<n>`. With neither, each command picks a free port (within `PORT_LO` to `PORT_HI` when both are set) and prints its URL: read that URL, and do not assume a port. A native client starts a second loop on `n+1`: keep that port free too.
5. A storage store keeps its data per port: drive one store on one port. `agent web --storage s1` starts empty each run. Add `--keep-storage` to reopen its saved data, or `--reset-storage` to reset it. A live drive owns its profile: another drive cannot reset it.
6. If Bun is not found: launchers prefer `$BUN`, then `~/.bun-1.4.2/bin/bun`, and `./exact.mjs` also works when PATH has no Bun. To select a Chromium, set `CHROME=/absolute/path/to/chrome`. Set a variable for the command only, and ask me before you change my shell profile.
7. Rerun the command that failed. For `web`, check that it serves at the printed URL. For `test web` or `agent web`, check the command's result. Report any failure that predates your changes or persists after three rounds of fixes, and record it as in step 8.
8. When a doc was wrong or missing, or a step needed a workaround, read `bun exact.mjs feedback status`. Under `never`, keep no note. Otherwise record it: `bun exact.mjs feedback note "<what happened>"`, which goes to Expo only as that status says. Do not change the policy; tell me how many notes you kept.
9. Tell me what held the port, the port and the URL the app uses now, and the command to run. Tell me what I must still do, if anything: stop another process, or set `BUN` or `CHROME` in my shell profile.
```
