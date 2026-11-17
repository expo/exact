import type { Answer, Result, Sources } from './app.contract.d.ts';

export const grants = '';

type Row = Result<'feed'>['rows'][number];
type Message = { id: string; body: string; mine: boolean; at: number };

// The window the transcript shows and how far a page moves it: many screens each, so the newest window stays
// taller than two of the tallest list's ports and its own end never reaches toward its start.
const WINDOW = 150, PAGE = 50, DAY = 86400000;
const WORDS = ['See you there', 'On my way', 'Running a little late', 'Sounds good', 'Lunch?', 'Back at six', 'Thanks!', 'Call me later'];

// The history, in memory: 600 messages over the twenty days before 2026-01-01, the tests' default date.
const history: Message[] = Array.from({ length: 600 }, (_, i) => ({
  id: String(i + 1),
  body: WORDS[i % WORDS.length],
  mine: i % 3 === 0,
  at: Date.UTC(2026, 0, 1) - (600 - i) * 48 * 60000,
}));

/** History[start, end) as rows, each day after the window's first message opened by a day row. */
function rows(start: number, end: number, offset: number): Row[] {
  const out: Row[] = [];
  let day: number | null = null;
  for (const m of history.slice(start, end)) {
    const d = Math.floor((m.at + offset * 60000) / DAY);
    if (d !== day) out.push({ id: `d-${d}`, body: '', mine: false, at: m.at, day: true });
    day = d;
    out.push({ ...m, day: false });
  }
  return out;
}

const sources: Sources = {
  // `cursor` is "" for the newest window, else its first message's index. Only the window is read: O(window).
  // `later` is an index too, never "": messages that arrive while the reader is back in the history move the
  // newest window on, and following it would skip them. `first` is the first message's id (a day row's is not one
  // message's), the row a page that lands keeps in place.
  feed: ([cursor, offset]) => {
    const n = history.length, last = Math.max(0, n - WINDOW);
    const start = cursor === '' ? last : Math.min(last, Math.max(0, Number(cursor)));
    const end = Math.min(n, start + WINDOW);
    return {
      rows: rows(start, end, offset),
      first: history[start]?.id ?? '',
      earlier: String(Math.max(0, start - PAGE)),
      later: String(Math.min(last, start + PAGE)),
      hasEarlier: start > 0,
      hasLater: end < n,
    };
  },
  incoming: ([at]) => {
    history.push({ id: String(history.length + 1), body: WORDS[history.length % WORDS.length], mine: false, at });
    return { ok: true };
  },
  sendMessage: ([body, at]) => {
    history.push({ id: String(history.length + 1), body, mine: true, at });
    return { ok: true };
  },
};
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
