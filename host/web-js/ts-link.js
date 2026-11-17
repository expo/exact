// What ts-data.js reads from the runtime besides rt-link.js's own objects, put on the link by an entry with a
// TypeScript data module, so that a page with none carries none of it and the seam can load as a chunk of its own
// (LLP 1047.003 D6).
import { checkpoint, inflight, Origin, painted, R, Resources, time } from './rt.js';
import { rt } from './rt-link.js';
Object.assign(rt, { checkpoint, inflight, Origin, painted, R, Resources, time });
