import { realpath, stat } from 'node:fs/promises';
import { extname, join, sep } from 'node:path';

const root = join(import.meta.dir, process.env.SERVE_DIST === '0' ? '.' : 'dist');
const port = Number(process.env.PORT || 4173);
const types = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.glb': 'model/gltf-binary', '.map': 'application/json', '.md': 'text/markdown' };

/** The regular file `pathname` names under `root`, after decoding and following
 * links; null when it names none there. Throws URIError on a malformed escape. */
async function inRoot(pathname) {
  const relative = decodeURIComponent(pathname === '/' ? '/index.html' : pathname);
  try {
    const base = await realpath(root);
    const path = await realpath(join(base, relative));
    if (!path.startsWith(base + sep) || !(await stat(path)).isFile()) return null;
    return path;
  } catch {
    return null;
  }
}

const server = Bun.serve({
  hostname: '127.0.0.1',
  port,
  async fetch(request) {
    let path;
    try { path = await inRoot(new URL(request.url).pathname); }
    catch { return new Response('Bad request', { status: 400 }); }
    if (!path) return new Response('Not found', { status: 404 });
    return new Response(Bun.file(path), { headers: { 'content-type': types[extname(path)] || 'application/octet-stream', 'cache-control': 'no-store' } });
  },
});

console.log(`Lanterns server http://127.0.0.1:${server.port} (${root})`);
