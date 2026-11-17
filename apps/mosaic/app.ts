import type { Answer, Result, Sources } from './app.contract.d.ts';
import { convex, ConvexFailure } from '@exact/convex';
import { getFunctionName } from 'convex/server';
import { convexToJson, jsonToConvex } from 'convex/values';
import { api } from './convex/_generated/api';

// The deployment's socket origin, where `npx convex dev` runs one on this machine (and where the tests'
// server.ts listens). A cloud deployment is 'wss://<name>.convex.cloud'.
const CONVEX = 'ws://127.0.0.1:3210';
export const grants = `net.websocket ${CONVEX}`;

const cx = convex(CONVEX, { getFunctionName, convexToJson, jsonToConvex });
// A refusal is a reply: Convex's own words when it refused the write, ours when it never went.
const reply = (write: Promise<unknown>): Promise<Result<'paint'>> => write.then(
  () => ({ ok: true, error: '' }),
  (e) => ({ ok: false, error: e instanceof ConvexFailure ? e.message : 'Mosaic had a problem. Try again.' }));

const sources: Sources = {
  live: () => cx.live(),
  // Convex's board is its tiles' hues in order; the view keys each tile by its place.
  board: ([name, rev]) => cx.query(rev, api.board.get, { name }, (b) => ({ tiles: b.cells.map((hue, at) => ({ at, hue })), strokes: b.strokes, painters: b.painters })),
  recent: ([name, rev]) => cx.query(rev, api.board.recent, { name }),
  paint: ([name, index, hue, by]) => reply(cx.mutation(api.board.paint, { name, index, hue, by })),
  clear: ([name]) => reply(cx.mutation(api.board.clear, { name })),
};
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
