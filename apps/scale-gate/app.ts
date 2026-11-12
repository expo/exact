import type { Answer } from './app.contract.d.ts';

export const grants = '';
export const answer: Answer = (source) => { throw new Error(`Unknown source: ${source}`); };
