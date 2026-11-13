import { realpath, stat } from "node:fs/promises";
import { join, sep } from "node:path";

const port = Number(process.env.PORT ?? 4173);

/** The regular file `pathname` names under `dist`, after decoding and following
 * links; null when it names none there. Throws URIError on a malformed escape. */
async function inDist(pathname) {
  const relative = decodeURIComponent(pathname === "/" ? "/index.html" : pathname);
  try {
    const base = await realpath("dist");
    const path = await realpath(join(base, relative));
    if (!path.startsWith(base + sep) || !(await stat(path)).isFile()) return null;
    return path;
  } catch {
    return null;
  }
}

const server = Bun.serve({
  hostname: "127.0.0.1",
  port,
  async fetch(request) {
    let path;
    try {
      path = await inDist(new URL(request.url).pathname);
    } catch {
      return new Response("bad request", { status: 400 });
    }
    if (!path) return new Response("not found", { status: 404 });
    const headers = new Headers();
    if (path.endsWith(".wasm")) headers.set("content-type", "application/wasm");
    if (path.endsWith(".js")) headers.set("content-type", "text/javascript");
    if (path.endsWith(".html")) headers.set("content-type", "text/html; charset=utf-8");
    return new Response(Bun.file(path), { headers });
  },
});
console.log(`Lanterns at http://127.0.0.1:${server.port}`);
