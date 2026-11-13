// Declared permission dependencies (LLP 1133.030 D12 item 1), as the runner's
// modules/permits.rs: shared own states, least effective state, required prompts first.
export function permissions(rows, states) {
  const declaredOf = row => row.key.slice(row.key.indexOf(".") + 1).split("#")[0];
  const permissionsOf = row => [row.sub.permission, ...Object.values(row.sub.permissionBy ?? {}), ...(row.sub.permissionList ?? [])].filter(Boolean);
  const declared = name => [...rows()].flatMap(permissionsOf).find(p => p.name === name);
  const required = p => p?.requires?.web ?? [];
  const initial = i => typeof i === "string" ? i : i?.web ?? i?.["*"];
  const ownState = (name, path) => {
    const p = declared(name);
    if (p?.grantedBy && !path.includes(p.grantedBy) && stateOf(p.grantedBy, path) === "granted") return "granted";
    return states.get(name) ?? initial(p?.initial) ?? "prompt";
  };
  const stateOf = (name, path = []) => {
    path = [...path, name];
    const rank = { denied: 0, prompt: 1, unknown: 2, granted: 3 };
    let state = ownState(name, path);
    for (const dependency of required(declared(name))) if (!path.includes(dependency)) {
      const other = stateOf(dependency, path);
      if (rank[other] < rank[state]) state = other;
    }
    return state;
  };
  const permissionNamed = (row, name) => permissionsOf(row).find(p => p.name === name);
  const statusesOf = row => permissionsOf(row).filter(p => p.status === declaredOf(row));
  const permissionOf = (row, args) => {
    const by = row.sub.permissionBy;
    if (by) {
      const asked = Object.entries(by).filter(([a]) => a === "*" || (args?.[a] != null && args[a] !== false)).map(([, p]) => p);
      return asked.find(p => stateOf(p.name) === "denied") ?? asked.find(p => stateOf(p.name) === "prompt") ?? asked[0] ?? row.sub.permission;
    }
    const own = row.sub.permission, names = required(own);
    if (!own || !names.length || statusesOf(row).length || ownState(own.name, [own.name]) === "denied") return own;
    const name = names.find(n => stateOf(n) === "denied") ?? names.find(n => stateOf(n) === "prompt");
    return name ? permissionNamed(row, name) : own;
  };
  const statusValue = (row, value) => {
    const out = structuredClone(value ?? {});
    for (const p of statusesOf(row)) {
      const parts = p.field.split("."), last = parts.pop();
      let at = out;
      for (const part of parts) at = at?.[part];
      const web = p.field.split(".").reduce((at, part) => at?.[part], row.sub.web);
      if (at && typeof at === "object") at[last] = web ?? stateOf(p.name);
    }
    return out;
  };
  return { stateOf, permissionOf, permissionsOf, permissionNamed, statusesOf, statusValue };
}
