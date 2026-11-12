/** A row key, including its occurrence when repeated, shared by DOM reconciliation and binding probes. */
export function rowKey(value, rows, seen) {
  if (!(typeof value === 'string' || typeof value === 'boolean' || typeof value === 'number' && Number.isFinite(value))) {
    throw new Error(`a row key that is not a string, finite number or bool: ${value}`);
  }
  const key = typeof value + ':' + (Object.is(value, -0) ? 0 : value);
  if (!rows?.has(key)) return key;
  const occurrence = seen.get(key) ?? 1;
  seen.set(key, occurrence + 1);
  return `d${occurrence}:${key}`;
}

/** The indices of `list`'s kept rows (`old`, their former places) on a
 * longest run in increasing former order: the rows that need not move. */
export function inOrder(list) {
  const tails = [], prev = new Array(list.length);
  for (let i = 0; i < list.length; i++) {
    const o = list[i].old;
    if (o === undefined) continue;
    let lo = 0, hi = tails.length;
    while (lo < hi) { const mid = (lo + hi) >> 1; if (list[tails[mid]].old < o) lo = mid + 1; else hi = mid; }
    prev[i] = lo ? tails[lo - 1] : -1;
    tails[lo] = i;
  }
  const stay = new Set();
  for (let i = tails.length ? tails[tails.length - 1] : -1; i >= 0; i = prev[i]) stay.add(i);
  return stay;
}
