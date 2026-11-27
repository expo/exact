// A page served beside the web host's own modules, for a browser test that imports them: `routes` maps a path to its
// body (a string, a Buffer, or a function that makes one) and type; `/js/names.js` is the empty build output `rt.js`
// reaches; and otherwise only `.js` files under `host/web` and `host/web-js` are served.
import { existsSync, readFileSync, realpathSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { resolve, sep } from 'node:path';

const WEB = resolve(import.meta.dirname, '..'), ROOTS = [WEB, resolve(WEB, '../web-js')].map(root => realpathSync(root) + sep);

/** Listens on a free local port; resolves to its URL and a `close`. */
export async function serveModules(routes) {
  const server = createServer((req, res) => {
    const route = routes[req.url] ?? (req.url === '/js/names.js' ? ['export const sourceTypes = {};', 'text/javascript'] : null);
    if (route) {
      const [body, type] = route;
      res.writeHead(200, { 'content-type': type });
      res.end(typeof body === 'function' ? body() : body);
      return;
    }
    // `/js/` is the JS target's directory, beside this one; its modules import ours as `../web/`
    const path = req.url.startsWith('/js/') ? req.url.slice(4) : req.url.replace(/^\/(web\/)?/, ''), js = resolve(WEB, '../web-js', path);
    const file = req.url.startsWith('/js/') && existsSync(js) ? js : resolve(WEB, path);
    const real = existsSync(file) && statSync(file).isFile() ? realpathSync(file) : null;
    if (!real?.endsWith('.js') || !ROOTS.some(root => real.startsWith(root))) { res.writeHead(404); res.end(); return; }
    res.writeHead(200, { 'content-type': 'text/javascript' });
    res.end(readFileSync(real));
  });
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  return { url: `http://127.0.0.1:${server.address().port}`, close: () => server.close() };
}

/** A module's route: the file read as served, so that a test names the module it plays. */
export const moduleRoute = file => [() => readFileSync(file), 'text/javascript'];
