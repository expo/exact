import type { Answer, Result } from './app.contract.d.ts';

export const appId = 'com.exact.windowframe';
export const grants = '';

// `call` asks the module synchronously, `later` as a long call.
export const answer: Answer = (async (source, args, _store, _storage, native) => {
  if (source !== 'frame' || !native?.available) throw { kind: 'UnknownSource', message: source };
  const [to, how] = args as [string, string];
  return (how === 'call' ? native.call({ to }) : await native.later({ to })) as Result<'frame'>;
}) as Answer;
