import { expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseSync } from 'rolldown/utils';
import { check, configure } from '../src/typescript.mjs';

test('generated json bindings narrow opaque values and retain known DOM dictionaries', async () => {
  const root = mkdtempSync(join(tmpdir(), 'exact-json-bindings-'));
  const app = join(root, 'app'), stage = join(root, 'stage');
  const libraries = resolve(dirname(fileURLToPath(import.meta.resolve(`@typescript/typescript-${process.platform}-${process.arch}/package.json`))), 'lib');
  try {
    mkdirSync(join(app, 'node_modules/@exact'), { recursive: true });
    mkdirSync(stage);
    for (const name of ['browser-automation', 'passkeys']) {
      symlinkSync(resolve(import.meta.dir, '../../../packages', name), join(app, 'node_modules/@exact', name), process.platform === 'win32' ? 'junction' : 'dir');
    }
    writeFileSync(join(stage, '__exact_paths.json'), JSON.stringify({ app, mounts: [] }));
    writeFileSync(join(stage, '__exact_entry.ts'), `
import { evaluate, callFunction } from '@exact/browser-automation';
import type { PublicKeyCredentialRequestOptionsJSON } from '@exact/passkeys';

export async function title() {
  const { result } = await evaluate({ id: 'guest', expression: 'document.title' });
  // @ts-expect-error An opaque result requires narrowing before property access.
  result.title;
  if (result === 'Exact') return result.toUpperCase();
  if (typeof result === 'string') return result.toLowerCase();
  return null;
}

export const called = callFunction({
  id: 'guest', functionDeclaration: 'value => value',
  arguments: ['text', 42, true, null, [], { title: 'Exact' }],
});

export const request: PublicKeyCredentialRequestOptionsJSON = {
  challenge: 'eA',
  allowCredentials: [{ id: 'eA', type: 'public-key', transports: ['usb'] }],
};
export const timeout: number | undefined = request.timeout;
// @ts-expect-error A known DOM dictionary retains its field types.
request.challenge = 42;
// @ts-expect-error A referenced DOM dictionary retains its field types.
request.allowCredentials![0].id = 42;
`);
    configure(stage, false, parseSync);
    await check(stage, join(libraries, 'tsc'), libraries);

    const binding = readFileSync(join(stage, '__exact_modules/exact__passkeys.ts'), 'utf8');
    const dom = readFileSync(join(libraries, 'lib.dom.d.ts'), 'utf8');
    for (const name of ['PublicKeyCredentialRequestOptionsJSON', 'PublicKeyCredentialDescriptorJSON', 'Base64URLString']) {
      const declaration = dom.match(new RegExp(`^(interface ${name}\\b[^}]*\\}|type ${name} = [^;]*;)`, 'm'))?.[1];
      expect(declaration).toBeDefined();
      expect(binding).toContain('export ' + declaration);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
