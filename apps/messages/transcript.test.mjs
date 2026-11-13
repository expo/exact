import { expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { connect } from 'node:net';
import { resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { fixtures } from './model.ts';

// Build first: `bun host/web/build.mjs messages` and
// `cargo build --profile host-dev -p messages-linux`.
for (const host of ['web', 'linux']) {
  test(`${host}: the Messages transcript mounts a window of a large conversation`, async () => {
    const cache = mkdtempSync(resolve('target/messages-transcript-'));
    const threads = fixtures();
    // At 12,000 rows the full native snapshot exceeds its 100 ms source budget.
    const count = host === 'web' ? 12000 : 1000, last = count - 1;
    threads[0].messages = Array.from({ length: count }, (_, i) => ({
      ...threads[0].messages[i % 3], id: `fixture-${i}`, fresh: false,
      body: i < 1000 ? `Row ${i}: ${'a variable height message. '.repeat(1 + i % 8)}` : `Row ${i}`,
      reaction: i === 500 || i === 501 ? '👍' : '',
    }));
    threads[1].messages = threads[0].messages.map(m => ({ ...m, id: `ai-${m.id}` }));
    const saved = JSON.stringify({ version: 1, threads, serial: count });
    let content = '', done = false, fail = false;
    const server = createServer((request, response) => {
      const headers = { 'Access-Control-Allow-Origin': '*', 'Access-Control-Allow-Headers': 'Content-Type',
        'Access-Control-Allow-Methods': 'GET, POST, DELETE, OPTIONS' };
      if (request.method === 'OPTIONS') { response.writeHead(204, headers); response.end(); return; }
      const path = new URL(request.url, 'http://127.0.0.1:4318').pathname;
      const value = path === '/health' ? { connected: true } : request.method === 'POST'
        ? (fail ? { error: 'Fixture service error' } : { id: 'fixture-turn' })
        : { content, done, error: '' };
      response.writeHead(fail && request.method === 'POST' ? 402 : 200,
        { ...headers, 'Content-Type': 'application/json' });
      response.end(JSON.stringify(value));
    });
    // Portable fetch uses CONNECT even for HTTP. Tunnel it to this fixture
    // so no request reaches the application's service port.
    server.on('connect', (request, socket, head) => {
      expect(request.url).toBe('127.0.0.1:4318');
      const upstream = connect(server.address().port, '127.0.0.1', () => {
        socket.write('HTTP/1.1 200 Connection Established\r\n\r\n');
        if (head.length) upstream.write(head);
        socket.pipe(upstream);
        upstream.pipe(socket);
      });
      upstream.on('error', () => socket.destroy());
      socket.on('error', () => upstream.destroy());
      socket.on('close', () => upstream.destroy());
    });
    const sockets = new Set();
    server.on('connection', socket => { sockets.add(socket); socket.on('close', () => sockets.delete(socket)); });
    await new Promise((resolve, reject) => server.once('error', reject).listen(0, '127.0.0.1', resolve));
    const endpoint = `http://127.0.0.1:${server.address().port}`;
    let session;
    try {
      if (host === 'linux') {
        const data = resolve(cache, 'exact/com.exact.messages/agent/transcript/data');
        mkdirSync(data, { recursive: true });
        writeFileSync(resolve(data, 'messages.json'), saved);
      }
      session = await open({ host, app: 'messages', size: [402, 874], storage: 'transcript',
        env: { XDG_CACHE_HOME: cache, EXACT_AGENT_EPHEMERAL_PORT: '1',
          ...(host === 'linux' ? { EXACT_NET_PROXY: '', HTTP_PROXY: endpoint, http_proxy: endpoint,
            HTTPS_PROXY: endpoint, https_proxy: endpoint, ALL_PROXY: endpoint, all_proxy: endpoint,
            NO_PROXY: '', no_proxy: '' } : {}) } });
      if (host === 'web') {
        await session.carrier.evaluate(`(async () => {
          const { createFileStore } = await import('/storage-fs.js');
          const { storageKey } = await import('/storage-environment.js');
          const store = createFileStore(storageKey('com.exact.messages'));
          await store.atomicWriteFile('app:/data/messages.json', new TextEncoder().encode(${JSON.stringify(saved)}));
          store.close();
        })()`);
        await session.carrier.call('Page.addScriptToEvaluateOnNewDocument', { source: `
          const browserFetch = globalThis.fetch.bind(globalThis);
          globalThis.fetch = async (input, init) => {
            const url = new URL(input instanceof Request ? input.url : input, location.href);
            if (url.origin !== 'http://127.0.0.1:4318') return browserFetch(input, init);
            const response = await browserFetch(${JSON.stringify(endpoint)} + url.pathname + url.search, init);
            Object.defineProperty(response, 'url', { value: url.href });
            return response;
          };
        ` });
        await session.carrier.reset({ keep: true });
      }
      await session.clock('+250');
      await session.clock('settle');
      await session.tap('conversation-demo');
      await session.clock('settle');
      expect((await session.state()).resources.data.count).toBe(count);
      const tree = async () => (await session.op({ op: 'tree' })).nodes;
      const rows = async () => (await tree()).filter(node => node.props?.testId?.startsWith('message-row-'));
      const ids = async () => (await rows()).map(node => node.props.testId);
      const window = async () => {
        const mounted = await rows();
        expect(mounted.length).toBeGreaterThan(0);
        expect(mounted.length).toBeLessThan(100);
        expect(new Set(mounted.map(node => node.props.testId)).size).toBe(mounted.length);
        const fixture = mounted.map(node => node.props.testId.match(/^message-row-fixture-(\d+)$/))
          .filter(Boolean).map(match => Number(match[1]));
        expect(fixture).toEqual(fixture.slice().sort((a, b) => a - b));
        for (let i = 1; i < fixture.length; i++) expect(fixture[i]).toBe(fixture[i - 1] + 1);
        for (const i of fixture) {
          expect((await session.find(`message-actions-fixture-${i}`)).props.accessibilityLabel)
            .toBe(threads[0].messages[i].body);
        }
        const layout = await session.layout();
        const port = layout.nodes.find(node => node.testId === 'transcript');
        const boxes = mounted.map(row => layout.nodes.find(node => node.id === row.id));
        expect(boxes[0].y).toBeLessThanOrEqual(port.y + 17);
        expect(boxes.at(-1).y + boxes.at(-1).h).toBeGreaterThanOrEqual(port.y + port.h - 17);
        for (let i = 1; i < boxes.length; i++) {
          expect(boxes[i].y - (boxes[i - 1].y + boxes[i - 1].h)).toBeLessThanOrEqual(14);
        }
      };
      const box = async id => (await session.layout(id)).node.space.viewport;
      const endVisible = async id => {
        const port = await box('transcript'), last = await box(`message-row-${id}`);
        expect(last.y + last.h).toBeLessThanOrEqual(port.y + port.h + 1);
        expect(last.y + last.h).toBeGreaterThan(port.y);
      };
      const operation = async id => {
        await session.tap('add');
        await session.tap(id);
        await session.clock('settle');
      };
      // The headless host exposes the reaction button's press; its secondary
      // mouse input is canvas-only. Both routes open the same message actions.
      const menu = id => host === 'linux' ? session.tap(`reaction-${id}`)
        : session.tap(`message-actions-${id}`, { contextmenu: true });
      await window();
      expect(await ids()).toContain(`message-row-fixture-${last}`);
      const oldLast = (await session.find(`message-row-fixture-${last}`)).id;
      await endVisible(`fixture-${last}`);
      await session.tap('transcript', { into: { key: 'message-fixture-0' } });
      await session.clock('settle');
      expect(await ids()).toContain('message-row-fixture-0');
      await session.tap('composer');
      await session.clock('settle');
      await endVisible(`fixture-${last}`);
      await operation('receive');
      await endVisible(`m-${count + 1}`);

      // Each distant visit evicts the previous window; returning builds the key anew.
      for (const index of [0, 500, last - 9]) {
        await session.tap('transcript', { into: { key: `message-fixture-${index}` } });
        await session.clock('settle');
        await window();
        expect(await ids()).toContain(`message-row-fixture-${index}`);
        if (index === 0) expect(await ids()).not.toContain(`message-row-fixture-${last}`);
      }
      await session.tap('transcript', { into: { key: `message-fixture-${last}` } });
      await session.clock('settle');
      expect((await session.find(`message-row-fixture-${last}`)).id).not.toBe(oldLast);
      await session.tap('transcript', { wheel: [0, -600] });
      await session.clock('settle');
      await window();

      await session.tap('transcript', { into: { key: 'message-fixture-500' } });
      await session.clock('settle');
      const anchor = await box('message-row-fixture-500');
      await operation('receive');
      expect(Math.abs((await box('message-row-fixture-500')).y - anchor.y)).toBeLessThan(1);
      expect(await ids()).not.toContain(`message-row-m-${count + 2}`);
      await menu('fixture-500');
      await session.tap('react-heart');
      await session.clock('settle');
      expect((await tree()).some(node => node.props?.text === '❤️')).toBe(true);
      await menu('fixture-500');
      await session.tap('delete-message');
      await session.clock('settle');
      expect(await ids()).not.toContain('message-row-fixture-500');
      expect(await ids()).toContain('message-row-fixture-501');

      // The transcript's horizontal pan still reveals timestamps on mounted rows.
      const port = await box('transcript');
      await session.tap('transcript', { drag: { from: [port.w - 2, port.h / 2], dx: -60, dy: 0, during: [async () => {
        expect((await session.state()).slots.pull).toBeGreaterThan(0);
      }] } });
      await session.clock('settle');
      expect((await session.state()).slots.pull).toBe(0);
      await menu('fixture-501');
      await session.tap('reply');
      expect((await session.state()).slots.replyBody).toBe(threads[0].messages[501].body);
      await session.clock('settle');
      await endVisible(`m-${count + 2}`);
      await session.type('composer', 'A reply from the virtualized transcript');
      await session.tap('transcript', { into: { key: 'message-fixture-501' } });
      await session.clock('settle');
      expect(await ids()).toContain('message-row-fixture-501');
      expect(await ids()).not.toContain(`message-row-m-${count + 2}`);
      await session.tap('send');
      await session.clock('settle');
      await session.clock('+1500');
      await session.clock('settle');
      await endVisible(`m-${count + 3}`);
      expect((await tree()).some(node => node.props?.text === threads[0].messages[501].body)).toBe(true);
      await operation('toggle-typing');
      expect((await tree()).some(node => node.props?.testId === 'typing-indicator')).toBe(true);
      await operation('toggle-typing');
      expect((await tree()).some(node => node.props?.testId === 'typing-indicator')).toBe(false);
      await session.screenshot(resolve(`target/messages-transcript-${host}.png`));

      await session.tap('back');
      await session.tap('conversation-ai-1');
      await session.clock('settle');
      await operation('earliest');
      expect(await ids()).toContain('message-row-ai-fixture-0');
      await operation('latest');
      await endVisible(`ai-fixture-${last}`);
      await session.type('composer', 'Stream into a large transcript');
      await session.tap('send');
      await session.clock('settle');
      expect((await tree()).some(node => node.props?.testId === 'typing-indicator')).toBe(true);
      content = 'First chunk';
      await session.clock('+250');
      await session.clock('settle');
      expect((await tree()).filter(node => node.props?.text === content)).toHaveLength(1);
      const response = (await session.state()).resources.data.latest;
      const responseView = (await session.find(`message-row-${response}`)).id;
      content = 'A longer cumulative answer. '.repeat(30);
      await session.clock('+250');
      await session.clock('settle');
      expect((await tree()).filter(node => node.props?.text === content)).toHaveLength(1);
      expect((await session.find(`message-row-${response}`)).id).toBe(responseView);
      await endVisible(response);
      done = true;
      await session.clock('+250');
      await session.clock('settle');
      fail = true;
      await session.type('composer', 'Keep this message after an error');
      await session.tap('send');
      await session.clock('settle');
      expect((await tree()).some(node => node.props?.text === 'Fixture service error')).toBe(true);
      expect((await tree()).some(node => node.props?.testId === 'retry')).toBe(true);
      fail = false;
      await session.tap('retry');
      await session.clock('settle');
      expect((await tree()).some(node => node.props?.testId === 'retry')).toBe(false);
      await session.tap('back');
      await session.tap('conversation-ai-2');
      await session.clock('settle');
      expect(await rows()).toHaveLength(0);
      expect((await tree()).some(node => node.props?.text ===
        'Start a conversation. Your messages are sent to this model through OpenRouter.')).toBe(true);
    } catch (error) {
      if (session) console.error((await session.op({ op: 'logs' })).lines.slice(-10));
      throw error;
    } finally {
      await session?.close();
      for (const socket of sockets) socket.destroy();
      await new Promise(resolve => server.close(resolve));
      rmSync(cache, { recursive: true, force: true });
    }
  }, 120000);
}
