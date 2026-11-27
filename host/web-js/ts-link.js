// What ts-data.js and admission.js read from the runtime besides rt-link.js's own objects, put on the link by an entry
// with a TypeScript data module, so that a page with none carries none of it and the seam can load as a chunk of its
// own (LLP 1047.003 D6). `canvasCommitted`: a capture's canvas, which only a data module asks for (a Rust-only page's
// entry puts it there itself, build.mjs).
import { canvasCommitted, checkpoint, inflight, Invocation, Origin, painted, Resources, sourceChanged, time } from './rt.js';
import { rt } from './rt-link.js';
Object.assign(rt, { canvasCommitted, checkpoint, inflight, Invocation, Origin, painted, Resources, sourceChanged, time });
