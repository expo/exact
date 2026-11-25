import { test } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseSync } from 'rolldown/utils';
import { check, configure } from '../../../js/bake/src/typescript.mjs';

test('a capture-only installation and a display-list caller both type-check', async () => {
  const root = mkdtempSync(join(tmpdir(), 'exact-capture-binding-'));
  const app = join(root, 'app'), stage = join(root, 'stage'), scope = join(app, 'node_modules/@exact');
  const lib = resolve(dirname(fileURLToPath(import.meta.resolve(`@typescript/typescript-${process.platform}-${process.arch}/package.json`))), 'lib');
  try {
    mkdirSync(scope, {recursive:true});
    mkdirSync(stage);
    writeFileSync(join(stage, '__exact_paths.json'), JSON.stringify({app, mounts:[]}));
    const install = name => symlinkSync(resolve(import.meta.dir, '../..', name), join(scope, name), process.platform === 'win32' ? 'junction' : 'dir');
    const checkEntry = async source => {
      writeFileSync(join(stage, '__exact_entry.ts'), source);
      await configure(stage, false, parseSync);
      await check(stage, join(lib, 'tsc'), lib);
    };
    install('screen-capture');
    const header = "import {captureScreens} from '@exact/screen-capture';\n";
    await checkEntry(header + "export const all = () => captureScreens({directory:'app:/tmp'});");
    install('window-management');
    await checkEntry(header + `
      import {getScreenDetails} from '@exact/window-management';
      export async function region() {
        const {currentScreen} = await getScreenDetails();
        return captureScreens({directory:'app:/tmp', screen:currentScreen, rect:{x:0,y:0,width:10,height:10}});
      }
    `);
  } finally { rmSync(root, {recursive:true, force:true}); }
});
