import type { Answer } from './app.contract.d.ts';

// The suite writes the id `exact new` gave the app.
export const appId = '__APP_ID__';
// The App Store's lookup is the Apple implementation's own request (LLP 1133.038 D3).
export const grants = 'net.fetch https://itunes.apple.com';

export const answer: Answer = (source) => { throw new Error(`unknown source ${source}`); };
