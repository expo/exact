import type { Answer } from './app.contract.d.ts';

export const appId = 'com.exact.menupick';
export const grants = '';
export const answer: Answer = ((source: string): never => {
  throw { kind: 'UnknownSource', message: source };
}) as unknown as Answer;
