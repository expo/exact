// A client for TypeScript 7.1's language server (`tsc --lsp --stdio`), enough to ask what an editor asks of a
// `.contract` file. Positions are 1-based lines and columns, as the Contract compiler reports them.
import { spawn } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { pathToFileURL, fileURLToPath } from 'node:url';

export async function open(tsc, root) {
  const child = spawn(tsc, ['--lsp', '--stdio'], { cwd: root, stdio: ['pipe', 'pipe', 'pipe'] });
  let buffered = Buffer.alloc(0), next = 1; const waiting = new Map(), published = new Map(), stderr = [];
  child.stderr.on('data', d => stderr.push(String(d)));
  child.stdout.on('data', chunk => {
    buffered = Buffer.concat([buffered, chunk]);
    for (;;) {
      const head = buffered.indexOf('\r\n\r\n'); if (head < 0) return;
      const length = Number(/Content-Length:\s*(\d+)/i.exec(buffered.subarray(0, head).toString('ascii'))[1]);
      if (buffered.length < head + 4 + length) return;
      const message = JSON.parse(buffered.subarray(head + 4, head + 4 + length).toString('utf8')); buffered = buffered.subarray(head + 4 + length);
      if (message.id !== undefined && message.method) send({ jsonrpc: '2.0', id: message.id, result: null }); // a request of the client's: answered with nothing
      else if (message.id !== undefined) { waiting.get(message.id)?.(message); waiting.delete(message.id); }
      else if (message.method === 'textDocument/publishDiagnostics') published.set(message.params.uri, message.params.diagnostics);
    }
  });
  const send = message => { const text = JSON.stringify(message); child.stdin.write(`Content-Length: ${Buffer.byteLength(text)}\r\n\r\n${text}`); };
  const request = (method, params) => new Promise((done, fail) => { const id = next++; waiting.set(id, m => m.error ? fail(new Error(`${method}: ${m.error.message}`)) : done(m.result)); send({ jsonrpc: '2.0', id, method, params }); setTimeout(() => fail(new Error(`${method}: no answer in 20 s\n${stderr.join('')}`)), 20000); });
  const notify = (method, params) => send({ jsonrpc: '2.0', method, params });
  await request('initialize', { processId: process.pid, rootUri: pathToFileURL(root).href, workspaceFolders: [{ uri: pathToFileURL(root).href, name: 'test' }],
    capabilities: { textDocument: { hover: { contentFormat: ['markdown', 'plaintext'] }, definition: {}, references: {}, rename: { prepareSupport: true }, diagnostic: {}, publishDiagnostics: {}, completion: {} }, workspace: { workspaceFolders: true } },
    initializationOptions: { runExternalCode: true } });
  notify('initialized', {});
  const at = (file, line, col) => ({ textDocument: { uri: pathToFileURL(file).href }, position: { line: line - 1, character: col - 1 } });
  const place = l => { const uri = l.uri ?? l.targetUri, r = l.range ?? l.targetSelectionRange; return [fileURLToPath(uri), r.start.line + 1, r.start.character + 1, r.end.character + 1]; };
  return {
    stderr,
    openFile(file, languageId = 'contract') { notify('textDocument/didOpen', { textDocument: { uri: pathToFileURL(file).href, languageId, version: 1, text: readFileSync(file, 'utf8') } }); },
    async definition(file, line, col) { const r = await request('textDocument/definition', at(file, line, col)); return (Array.isArray(r) ? r : r ? [r] : []).map(place); },
    async references(file, line, col) { const r = await request('textDocument/references', { ...at(file, line, col), context: { includeDeclaration: true } }); return (r ?? []).map(place).sort((a, b) => a[1] - b[1] || a[2] - b[2]); },
    async hover(file, line, col) { const r = await request('textDocument/hover', at(file, line, col)); const c = r?.contents; return !c ? null : typeof c === 'string' ? c : Array.isArray(c) ? c.map(x => x.value ?? x).join('\n') : c.value; },
    async rename(file, line, col, newName) { const r = await request('textDocument/rename', { ...at(file, line, col), newName }); const edits = r?.changes ? Object.entries(r.changes).flatMap(([uri, es]) => es.map(e => [fileURLToPath(uri), e.range.start.line + 1, e.range.start.character + 1, e.newText])) : (r?.documentChanges ?? []).flatMap(c => c.edits.map(e => [fileURLToPath(c.textDocument.uri), e.range.start.line + 1, e.range.start.character + 1, e.newText])); return edits.sort((a, b) => a[1] - b[1] || a[2] - b[2]); },
    async diagnostics(file) { try { const r = await request('textDocument/diagnostic', { textDocument: { uri: pathToFileURL(file).href } }); return (r?.items ?? []).map(d => [d.range.start.line + 1, d.range.start.character + 1, d.message]); } catch { return (published.get(pathToFileURL(file).href) ?? []).map(d => [d.range.start.line + 1, d.range.start.character + 1, d.message]); } },
    request, close() { child.kill(); },
  };
}
