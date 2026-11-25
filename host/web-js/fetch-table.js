import { digest } from './sha256.js';

/** Race HTTP and the offline copies. Verify the response against its content name before using or saving it. */
export async function fetchTable(url, signal) {
  const cache = (async () => caches.open('exact.tables'))();
  const key = 'exact.table.' + url;
  const decode = async bytes => {
    const hex = (await digest(bytes)).slice(0, 12);
    const hash = btoa(String.fromCharCode(...hex.match(/../g).map(n => parseInt(n, 16)))).replaceAll('+', '-').replaceAll('/', '_');
    if (!url.endsWith('-' + hash + '.json')) throw new Error(`wrong content for ${url}`);
    return JSON.parse(new TextDecoder().decode(bytes));
  };
  const body = async response => {
    if (!response?.ok) throw new Error(response ? `${url}: HTTP ${response.status}` : 'table not cached');
    return new Uint8Array(await response.arrayBuffer());
  };
  const network = (async () => {
    // Even an explicitly cacheable HTTP error from an earlier release must reach the repaired server.
    const bytes = await body(await fetch(url, {cache: 'no-store', signal}));
    const value = await decode(bytes);
    // The page may close as soon as it paints. Keep the bytes until the asynchronous cache has committed them.
    try { localStorage.setItem(key, new TextDecoder().decode(bytes)); } catch {}
    cache.then(c => c.put(url, new Response(bytes))).then(() => localStorage.removeItem(key)).catch(() => {});
    return value;
  })();
  const staged = async () => {
    const text = localStorage.getItem(key);
    if (text === null) throw new Error('no pending cache write');
    return decode(new TextEncoder().encode(text));
  };
  try { return await Promise.any([network, cache.then(c => c.match(url)).then(body).then(decode), staged()]); }
  catch (error) { throw error.errors[0]; }
}
