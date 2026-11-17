import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { cdpBrowser } from '../../../scripts/agent-launch.mjs';
import { readyPage } from '../../../host/web/tests/page.mjs';

test('the web fixture writes every display, cancels before publication, and never opens a picker', async () => {
  const source = readFileSync(new URL('../web/index.js', import.meta.url), 'utf8');
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    if (new URL(request.url).pathname === '/capture.js') return new Response(source, { headers: { 'content-type':'text/javascript' } });
    return new Response('<script>window.ready=true</script>', { headers: { 'content-type':'text/html' } });
  } });
  const browser = await cdpBrowser();
  try {
    const { sessionId } = await browser.page();
    await readyPage(browser.cdp, sessionId, server.url.href);
    const { result, exceptionDetails } = await browser.cdp.send('Runtime.evaluate', { awaitPromise: true, returnByValue: true, expression: `
      (async () => {
        const { implementation } = await import('/capture.js');
        const files = new Map(); let writes = 0;
        const put = async (path, bytes) => { writes++; files.set(path,bytes); };
        const context = { agent:true, files:{ put, putAll: async entries => {
          for (const {path, blob} of entries) await put(path, blob);
        } } };
        const capture = implementation(context);
        const screens = [{ left:0, top:0, width:64, height:40, devicePixelRatio:1.25 },
          { left:-80, top:-10, width:80, height:40, devicePixelRatio:2 }];
        const answer = await capture.captureScreens({ directory:'app:/tmp/freeze' }, { request:1, preferred:{screens} });
        const bitmap = await createImageBitmap(files.get('app:/tmp/freeze/screen-0.png'));
        const canvas = new OffscreenCanvas(bitmap.width,bitmap.height), ctx=canvas.getContext('2d'); ctx.drawImage(bitmap,0,0);
        const pixel = Array.from(ctx.getImageData(0,0,1,1).data);
        const pending = capture.captureScreens({directory:'app:/tmp/freeze'}, {request:2}); capture.cancel(2);
        const cancelled = await pending;
        capture.preferred({screens:[{width:100000,height:100000,devicePixelRatio:1}]});
        const quota = await capture.captureScreens({directory:'app:/tmp/freeze'}, {request:3});
        capture.preferred({screens});
        const stored = files.get('app:/tmp/freeze/screen-0.png');
        const duringWrite = [];
        for (const end of ['cancel', 'destroy']) {
          let started, resume;
          const writing = new Promise(resolve => { started = resolve; });
          const held = new Promise(resolve => { resume = resolve; });
          const putAll = async (entries, refusal) => {
            started(); await held;
            const error = refusal?.(); if (error) throw error;
            for (const {path, blob} of entries) files.set(path, blob);
          };
          const delayed = implementation({ agent:true, files:{
            put:(path, blob, refusal) => putAll([{path, blob}], refusal), putAll,
          } });
          const result = delayed.captureScreens({directory:'app:/tmp/freeze'}, {request:4,preferred:{screens}});
          await writing;
          if (end === 'cancel') delayed.cancel(4); else delayed.destroy();
          resume();
          duringWrite.push({ answer:await result, preserved:files.get('app:/tmp/freeze/screen-0.png') === stored });
        }
        const betweenWrites = [];
        for (const end of ['cancel', 'destroy']) {
          const paths = [0, 1].map(n => 'app:/tmp/freeze/screen-' + n + '.png');
          const old = paths.map(path => files.get(path));
          const interrupted = implementation({ agent:true, files:{
            put:async (path, blob, refusal) => {
              const error = refusal(); if (error) throw error;
              files.set(path, blob);
              if (end === 'cancel') interrupted.cancel(5); else interrupted.destroy();
            },
            putAll:async (entries, refusal) => {
              const error = refusal(); if (error) throw error;
              for (const {path, blob} of entries) files.set(path, blob);
              if (end === 'cancel') interrupted.cancel(5); else interrupted.destroy();
            },
          } });
          const answer = await interrupted.captureScreens({directory:'app:/tmp/freeze'}, {request:5,preferred:{screens}});
          betweenWrites.push({ error:answer.error ?? null,
            replaced: paths.map((path, i) => files.get(path) !== old[i]) });
        }
        const real = implementation({...context,agent:false});
        return { answer, pixel, cancelled, quota, writes, duringWrite, betweenWrites,
          native:await real.captureScreens({directory:'app:/tmp/x'}) };
      })()` }, sessionId);
    expect(exceptionDetails).toBeUndefined();
    expect(result.value.answer.captures.map(c => [c.width,c.height])).toEqual([[160,80],[80,50]]);
    expect(result.value.pixel).toEqual([32,64,128,255]);
    expect(result.value.cancelled.error).toBe('AbortError');
    expect(result.value.quota.error).toBe('QuotaExceededError');
    expect(result.value.native.error).toBe('NotSupportedError');
    expect(result.value.writes).toBe(2);
    expect(result.value.duringWrite.map(r => [r.answer.error, r.preserved])).toEqual([
      ['AbortError', true], ['AbortError', true],
    ]);
    expect(result.value.betweenWrites).toEqual([
      { error:null, replaced:[true, true] }, { error:null, replaced:[true, true] },
    ]);
  } finally { await browser.close(); server.stop(true); }
}, 60000);
