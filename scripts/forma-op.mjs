// `bun exact.mjs forma op`: one request to a design server, or its change
// stream (moved from scripts/forma-server.mjs, under its line cap).

/** `bun exact.mjs forma op <url> <request JSON>|subscribe`: the scripted
 * client. It takes the token as any local client does, from that origin and
 * no other (no redirect is followed), then prints the reply, or each event
 * as it comes. */
export async function op(argv) {
  const [base, what] = argv;
  if (!base || !what) { console.error('usage: bun exact.mjs forma op <server url> <request JSON>|subscribe'); return 2; }
  const origin = new URL(base).origin;
  const got = await fetch(`${origin}/__forma/token`, { redirect: 'error' });
  if (!got.ok) { console.error(`forma op: ${origin} gave no token (${got.status}); it answers this machine's loopback only`); return 1; }
  const { token } = await got.json();
  const headers = { 'content-type': 'application/json', 'x-exact-forma-token': token };
  if (what === 'subscribe') {
    const res = await fetch(`${origin}/__forma/subscribe`, { method: 'POST', headers, redirect: 'error' });
    if (!res.ok) { console.error(`forma op: ${await res.text()}`); return 1; }
    const decoder = new TextDecoder();
    let buf = '';
    for await (const chunk of res.body) {
      buf += decoder.decode(chunk, { stream: true });
      const parts = buf.split('\n\n'); buf = parts.pop();
      for (const part of parts) if (part.startsWith('data: ')) console.log(part.slice(6));
    }
    return 0;
  }
  const res = await fetch(`${origin}/__forma`, { method: 'POST', headers, body: what, redirect: 'error' });
  console.log(await res.text());
  return res.ok ? 0 : 1;
}
