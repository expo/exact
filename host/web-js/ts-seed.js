// Under the agent driver on a development page, initialize the deterministic
// ChaCha20 random streams synchronously before app.ts evaluates so both
// top-level `Math.random()` (`ts-init`) and in-answer `Math.random()`
// (`ts-random`) stay deterministic and isolated from `crypto.randomUUID()`
// (`typescript`) (LLP 1195.000 D2.5).
import { answering } from './ts-fetch.js';
import { agentMode } from './rt.js';
import { agentSeed, agentStream } from './storage-environment.js';

if (agentMode() && !globalThis.__exactRender) {
  const seed = agentSeed();
  if (seed !== null) {
    answering.entropy = Object.assign(agentStream(seed, 'typescript'), { seed });
    answering.random = Object.assign(agentStream(seed, 'ts-random'), { seed });
    answering.randomInit = Object.assign(agentStream(seed, 'ts-init'), { seed });
  }
}
