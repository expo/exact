// Mosaic's Convex functions: a board of tiles, and the strokes that painted it.
// `paint` writes both in one transaction, so `get` and `recent` change together.
import { v } from 'convex/values';
import { internal } from './_generated/api';
import { internalMutation, mutation, query, type MutationCtx, type QueryCtx } from './_generated/server';
import { blank, paint as painted, RECENT, summary } from './tiles';

const named = (ctx: QueryCtx, name: string) => ctx.db.query('boards').withIndex('by_name', (q) => q.eq('name', name)).unique();

export const get = query({
  args: { name: v.string() },
  handler: async (ctx, { name }) => summary((await named(ctx, name)) ?? blank()),
});

// Only the strokes of the board as it is now: a Clear deletes the board at
// once and its strokes in batches after it, and the first stroke after a
// Clear makes the board again.
export const recent = query({
  args: { name: v.string() },
  handler: async (ctx, { name }) => {
    const board = await named(ctx, name);
    if (!board) return [];
    return (await ctx.db.query('strokes').withIndex('by_board', (q) => q.eq('board', name).gte('_creationTime', board._creationTime)).order('desc').take(RECENT))
      .map((s) => ({ id: s._id as string, by: s.by, index: s.index, hue: s.hue }));
  },
});

export const paint = mutation({
  args: { name: v.string(), index: v.number(), hue: v.number(), by: v.string() },
  handler: async (ctx, { name, ...stroke }) => {
    const board = await named(ctx, name);
    const next = painted(board ?? blank(), stroke);
    if (board) await ctx.db.patch(board._id, next); else await ctx.db.insert('boards', { name, ...next });
    await ctx.db.insert('strokes', { board: name, ...stroke });
    return null;
  },
});

// A mutation's writes are capped (https://docs.convex.dev/production/state/limits),
// and the log keeps every stroke, so it is deleted in batches, each scheduling
// the next. A batch deletes only strokes made by the Clear's time, never one
// painted on the board after it.
const BATCH = 1000;
async function sweep(ctx: MutationCtx, name: string, through: number) {
  const batch = await ctx.db.query('strokes')
    .withIndex('by_board', (q) => q.eq('board', name).lte('_creationTime', through)).take(BATCH);
  for (const s of batch) await ctx.db.delete(s._id);
  if (batch.length === BATCH) await ctx.scheduler.runAfter(0, internal.board.sweepStrokes, { name, through });
}

export const clear = mutation({
  args: { name: v.string() },
  handler: async (ctx, { name }) => {
    const board = await named(ctx, name);
    if (board) await ctx.db.delete(board._id);
    const newest = await ctx.db.query('strokes').withIndex('by_board', (q) => q.eq('board', name)).order('desc').first();
    if (newest) await sweep(ctx, name, newest._creationTime);
    return null;
  },
});

export const sweepStrokes = internalMutation({
  args: { name: v.string(), through: v.number() },
  handler: (ctx, { name, through }) => sweep(ctx, name, through),
});
