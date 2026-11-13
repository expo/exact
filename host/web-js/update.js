// A keyed list whose entries change, on the JS target (LLP 1133.033 D17 item 7, D21), as the runner's
// `modules/declared/update.rs`: a call the app places is listed, then its state moves as the app reports it
// (`"update"` on a send) and as the person acts on the system's screen (`tap @ref <choice>`). The forms are that
// file's: a list's `with`, `absent`, `stamp`, `refuse`, `ends`, `tap.choices`; a value in `set` is a literal,
// `{now: true}`, or `{from, map, absent}`. Due times, pause, resume and rearming a choice follow
// modules/declared/app_list.rs (LLP 1133.030 D12 item 2).

/** `lists`: the declared lists by state, app lists also by extension; `rows()` the module rows;
 * `now()` the agent clock's date; `changed(state)`
 * re-asks the sources that answer from a list; `raise(row, event, tap, fields)` raises an entry's event as an
 * activation, with the arguments `tap` declares of its `fields`. */
export function updates({ lists, rows, say, now, changed, raise }) {
  // What every row of list `state` declares of it, the first declaration of each key winning.
  const stateOf = (row, state) => lists.state?.(row, state) ?? state;
  const config = state => {
    const out = {};
    for (const row of rows()) if (row.sub.list && stateOf(row, row.sub.list.state) === state) for (const [k, v] of Object.entries(row.sub.list)) if (!(k in out)) out[k] = v;
    return out;
  };
  const matches = (fields, rule) => !!rule && typeof rule === "object"
    && Object.entries(rule).every(([field, want]) => Array.isArray(want) ? want.some(v => v === (fields[field] ?? null)) : want === (fields[field] ?? null));
  const prune = state => { const ends = config(state).ends; if (ends && lists.has(state)) lists.set(state, lists.get(state).filter(e => !matches(e.fields, ends))); };
  const valueOf = (value, args) => {
    if (!value || typeof value !== "object" || Array.isArray(value)) return [value];
    if (value.now === true) return [now()];
    if (typeof value.from !== "string") return [value];
    const given = args[value.from] ?? value.absent ?? null;
    if (!value.map) return [given];
    const word = typeof given === "string" ? given : JSON.stringify(given);
    return Object.hasOwn(value.map, word) ? valueOf(value.map[word], args) : [];
  };
  const setFields = (fields, set, args) => { for (const [field, v] of Object.entries(set ?? {})) { const got = valueOf(v, args); if (got.length) fields[field] = got[0]; } };
  const refusal = (update, args, row) => {
    const entry = (lists.get(stateOf(row, update.state ?? "listed")) ?? []).find(e => e.fields[update.key] === (args[update.key] ?? null));
    return !entry || (update.when && !matches(entry.fields, update.when)) ? { error: update.else ?? "NotFoundError" } : null;
  };
  return {
    /** The listing's own forms over the call's `fields` before it is listed: the refusal's code, or null. */
    listing(list, fields, row) {
      const state = stateOf(row, list.state ?? "listed");
      prune(state);
      if (list.refuse && list.key && (lists.get(state) ?? []).some(e => e.fields[list.key] === (fields[list.key] ?? null))) return list.refuse;
      for (const [k, v] of Object.entries(list.absent ?? {})) if (fields[k] == null) fields[k] = v;
      Object.assign(fields, list.with ?? {});
      if (list.stamp) fields[list.stamp] = now();
      const due = list.due;
      if (due?.into) {
        const at = fields[due.at], after = fields[due.after];
        const form = typeof at === "number" ? "at" : typeof after === "number" ? "after" : null;
        if (form) {
          fields[due.into] = Math.round(form === "at" ? at : now() + after);
          Object.assign(fields, due.as?.[form] ?? {}, { delivered: false });
        }
      }
      return null;
    },
    refusal,
    /** The entries due by epoch instant `at` fire, once; their lists are announced. */
    due(at) {
      const moved = [];
      for (const [state, entries] of lists) {
        const due = config(state).due;
        if (!due?.into) continue;
        const fired = entries.filter(e => e.fields.delivered === false && typeof e.fields[due.into] === "number" && e.fields[due.into] <= at);
        for (const e of fired) {
          e.fields = { ...e.fields, ...due.fires, delivered: true };
          say(`${e.row.sub.list.state} @${e.ref}: due`);
        }
        if (fired.length) moved.push(state);
      }
      for (const state of moved) changed(state);
    },
    /** `"update"` on a send: null, or `{error}`. */
    update(name, update, args, row) {
      const state = stateOf(row, update.state ?? "listed"), key = update.key ?? "", code = update.else ?? "NotFoundError", value = args[key] ?? null;
      const entry = (lists.get(state) ?? []).find(e => e.fields[key] === value);
      if (!entry) { say(`${name}: refused: ${code} (no entry ${JSON.stringify(value)})`); return { error: code }; }
      if (update.when && !matches(entry.fields, update.when)) { say(`${name}: refused: ${code} (${JSON.stringify(value)} is not ${JSON.stringify(update.when)})`); return { error: code }; }
      entry.fields = { ...entry.fields };
      const into = config(state).due?.into ?? "";
      if (update.pause) {
        const at = typeof entry.fields[into] === "number" ? entry.fields[into] : now();
        entry.fields[update.pause] = Math.round(Math.max(0, at - now()));
        entry.fields[into] = null;
      }
      if (update.resume) {
        const left = typeof entry.fields[update.resume] === "number" ? entry.fields[update.resume] : 0;
        entry.fields[into] = Math.round(now() + left);
        entry.fields[update.resume] = null;
      }
      setFields(entry.fields, update.set, args);
      say(`${name}: updated`);
      changed(state);
      return null;
    },
    /** Whether the entry's list declares choices: its tap then takes one. */
    chooses: entry => !!config(stateOf(entry.row, entry.row.sub.list.state)).tap?.choices,
    /** `tap @ref <choice>`: the person's act on the system's screen. */
    choose(entry, choice) {
      const state = stateOf(entry.row, entry.row.sub.list.state), choices = config(state).tap?.choices;
      if (!choices) return { error: `@${entry.ref} takes no choice: tap @${entry.ref} alone` };
      const chosen = Object.hasOwn(choices, choice) ? choices[choice] : null;
      if (!chosen) return { error: `@${entry.ref} takes ${Object.keys(choices).join(", ")}, not \`${choice}\`` };
      if (entry.fields.delivered === false) return { error: `@${entry.ref} is not delivered yet: advance the clock past its time` };
      if (chosen.retire) {
        lists.set(state, (lists.get(state) ?? []).filter(e => e.ref !== entry.ref));
      } else {
        if (chosen.when && !matches(entry.fields, chosen.when)) {
          const now = Object.keys(chosen.when).map(f => `its ${f} is ${JSON.stringify(entry.fields[f] ?? null)}`);
          return { error: `@${entry.ref}: \`${choice}\` is refused: ${now.join(", ")}` };
        }
        entry.fields = { ...entry.fields };
        if (typeof chosen.after === "string") {
          const after = entry.fields[chosen.after];
          if (typeof after !== "number") return { error: `@${entry.ref} has no \`${chosen.after}\` for \`${choice}\`` };
          entry.fields[config(state).due?.into ?? ""] = Math.round(now() + after);
          entry.fields.delivered = false;
          Object.assign(entry.fields, chosen.set ?? {});
        } else setFields(entry.fields, chosen.set, {});
      }
      say(`${entry.row.sub.list.state} @${entry.ref}: ${choice}`);
      changed(state);
      if (typeof chosen.event !== "string") return { ref: entry.ref, choice, delivery: "substituted" };
      raise(entry.row, chosen.event, chosen, entry.fields);
      return { ref: entry.ref, choice, event: chosen.event, delivery: "substituted" };
    },
  };
}

// The declared lists, by state. An app-scoped list is keyed by its extension too, as the runner's
// `app_key` keeps it, so two extensions' lists of one state stay apart. The page runs one session
// of the app, so its app-scoped lists are the page's, as `set`'s app scope is here.
export function declaredLists({ rows }) {
  const lists = new Map();
  lists.state = (row, name) => {
    const extension = row.key.split(".")[0];
    const declared = [...rows()].find(r => r.key.split(".")[0] === extension && r.sub.list?.state === name);
    return declared?.sub.list.scope === "app" ? `${extension}/${name}` : name;
  };
  return lists;
}
