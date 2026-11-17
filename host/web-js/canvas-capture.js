// The two source languages share the commit boundary, encoder and refusals (LLP 1056 D9).
import { rt } from './rt-link.js';

// `type` and `quality` are `toBlob`'s: the browser writes a type it does not encode as a PNG, and the answer's `type`
// names what it wrote.
export async function captureCanvas(surface, path, write, ready = rt.canvasCommitted(surface), active = () => true) {
  const refuse = (name, why) => { throw new DOMException(`captureCanvas: ${why}`, name); };
  if (typeof surface !== 'string' || typeof path !== 'string') throw new TypeError('captureCanvas(surface, path, type, quality): a surface name and an app:/ path');
  const parts = path.startsWith('app:/') ? path.slice(5).split('/').filter(Boolean) : [];
  if (new TextEncoder().encode(path).length > 4096 || path.endsWith('/') || parts.length < 2
      || !['data', 'cache', 'tmp'].includes(parts[0]) || parts.some(p => p === '.' || p === '..' || p.includes('\0')))
    refuse('NotAllowedError', `${JSON.stringify(path)} is not an app:/ file`);
  const check = () => { if (!active()) refuse('AbortError', "the capture's ticket is no longer held"); };
  check();
  const c = await ready;
  check();
  if (c.name) refuse(c.name, c.message);
  let bytes, type;
  try { const blob = await c.encoded(); type = blob.type || 'image/png'; bytes = new Uint8Array(await blob.arrayBuffer()); }
  catch (e) { refuse('EncodingError', e?.message ?? e); }
  check();
  try { await write(path, bytes); }
  catch (e) { refuse(e?.code === 'denied' || /^denied: /.test(e?.message ?? '') ? 'NotAllowedError' : e?.code === 'agent' ? 'NotSupportedError' : 'EncodingError', `${path}: ${e?.message ?? e}`); }
  return { path, width: c.width, height: c.height, scale: c.scale, type };
}

// Both source languages publish under the owning ticket and the source's grants.
export async function writeCanvas(appId, grants, path, bytes, active) {
  const { bodyTarget } = await import(new URL('./storage-fs.js', import.meta.url).href);
  const target = await bodyTarget(appId, grants, path);
  const refusal = () => active() ? undefined : new DOMException("the capture's ticket is no longer held", 'AbortError');
  return target.put(new Response(bytes), bytes.byteLength, null, refusal, refusal);
}
