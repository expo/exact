import { expect, test } from 'bun:test';
import { open } from '../../scripts/agent.mjs';
import { checkOp } from '../../scripts/agent-check.mjs';

// Build first with `bun host/web/build.mjs messages`.
test('conversation controls, bubbles and menus stay readable in both appearances', async () => {
  const session = await open({ host: 'web', app: 'messages', size: [420, 900],
    storage: `q1-messages-${process.pid}`, env: { EXACT_AGENT_EPHEMERAL_PORT: '1' } });
  const failures = [];
  const check = async screen => {
    const result = await checkOp(session, ['both']);
    for (const report of result.check) {
      failures.push(...report.findings.filter(f => f.kind === 'contrast')
        .map(f => ({ screen, scheme: report.scheme, ...f })));
    }
  };
  try {
    await session.clock('settle');
    await check('inbox');
    await session.tap('conversation-demo');
    await session.clock('settle');
    await check('conversation');
    await session.type('composer', 'A draft');
    await check('send');
    await session.tap('message-actions-demo-1', { contextmenu: true });
    await session.clock('settle');
    await check('menu');
    await session.tap('reply');
    await session.clock('settle');
    await check('reply');
    await session.tap('add');
    await session.clock('settle');
    await check('actions');
    expect(failures).toEqual([]);
  } finally {
    await session.close();
  }
}, 30000);
