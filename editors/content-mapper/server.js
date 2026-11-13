// The mapper's process: `Content-Length`-framed JSON-RPC 2.0 over stdio, as TypeScript 7.1 speaks it
// (typescript-go internal/contentmapper). TypeScript is the only requester; stdout carries protocol messages alone.
import { createMapper } from './mapper.js';

const mapper = createMapper();
let buffered = Buffer.alloc(0);
const reply = (id, body) => { const text = JSON.stringify({ jsonrpc: '2.0', id, ...body }); process.stdout.write(`Content-Length: ${Buffer.byteLength(text)}\r\n\r\n${text}`); };
process.stdin.on('data', chunk => {
  buffered = Buffer.concat([buffered, chunk]);
  for (;;) {
    const head = buffered.indexOf('\r\n\r\n'); if (head < 0) return;
    const length = Number(/^Content-Length:\s*(\d+)\s*$/im.exec(buffered.subarray(0, head).toString('ascii'))?.[1]);
    if (!Number.isFinite(length)) { console.error('contract content mapper: a message without Content-Length'); process.exit(1); }
    if (buffered.length < head + 4 + length) return;
    const body = buffered.subarray(head + 4, head + 4 + length).toString('utf8');
    buffered = buffered.subarray(head + 4 + length);
    let message; try { message = JSON.parse(body); } catch { console.error('contract content mapper: a message that is not JSON, dropped'); continue; }
    if (message.id === undefined || message.id === null) continue; // a notification
    try {
      const method = mapper[message.method];
      if (!method) reply(message.id, { error: { code: -32601, message: `no method ${message.method}` } });
      else reply(message.id, { result: method(message.params ?? {}) ?? null });
    } catch (error) { console.error(error); reply(message.id, { error: { code: -32603, message: String(error?.message ?? error) } }); }
  }
});
process.stdin.on('end', () => process.exit(process.exitCode ?? 0));
