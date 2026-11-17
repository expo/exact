import type { Answer, Sources } from './app.contract.d.ts';

// The suite writes the grant lines it builds with.
export const grants = '__GRANTS__';

const sources: Sources = {};

export const answer: Answer = (source, args, store, storage) => sources[source](args, store, storage);
