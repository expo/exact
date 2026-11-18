import { expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { parseSync } from 'rolldown/utils';
import { rolldown } from 'rolldown';
import { runInNewContext } from 'node:vm';
import { configure, dataBundle, staticFacts } from '../src/typescript.mjs';

function fixture(files, manifest = {}) {
  const root = resolve(import.meta.dir, '../../../target');
  mkdirSync(root, { recursive: true });
  const stage = mkdtempSync(resolve(root, 'grant-constants-'));
  const put = (name, code) => {
    const file = resolve(stage, name);
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, code);
  };
  for (const [name, code] of Object.entries(files)) put(name, code);
  put('app.json', JSON.stringify(manifest));
  put('__exact_entry.ts', "export * from './app';\n");
  return { stage, put, close: () => rmSync(stage, { recursive: true, force: true }) };
}

test('grants fold imported literal arrays and spreads without running their module', async () => {
  const f = fixture({
    'app.ts': "import { SECRET_GRANTS } from './src/session';\nexport const grants = ['net.fetch https://api.example.com', ...SECRET_GRANTS].join('\\n');\n",
    'src/session.ts': "throw new Error('the static reader must not run this');\nexport const SECRET_GRANTS = ['secret.keep session.refresh', 'secret.keep session.device'];\n",
  });
  try {
    await configure(f.stage, false, parseSync);
    expect(staticFacts(readFileSync(resolve(f.stage, 'app.ts'), 'utf8'), parseSync).grants).toBe(
      'net.fetch https://api.example.com\nsecret.keep session.refresh\nsecret.keep session.device');
  } finally { f.close(); }
});

const constants = {
  'strings.ts': "export const PREFIX = 'secret.keep ';\nexport const NAME = 'session.refresh';\nexport const LINE = PREFIX + NAME;\nexport const LINES = [LINE, `secret.keep session.device`];\n",
  'barrel.ts': "export { PREFIX, NAME as REFRESH, LINE, LINES } from './strings';\n",
};
for (const stars of ["export * from './strings'; export * from './more';", "export * from './more'; export * from './strings';"]) {
  test(`star export cycles keep searching: ${stars}`, async () => {
    const f = fixture({ ...constants,
      'app.ts': "import { LINE } from './barrel'; export const grants = LINE;",
      'barrel.ts': stars, 'more.ts': "export * from './barrel'; export const EXTRA = 1;",
    });
    try {
      await configure(f.stage, false, parseSync);
      expect(staticFacts(readFileSync(resolve(f.stage, 'app.ts'), 'utf8'), parseSync).grants).toBe('secret.keep session.refresh');
    } finally { f.close(); }
  });
}

test('a star export never re-exports default', async () => {
  const f = fixture({
    'app.ts': "import LINE from './barrel'; export const grants = LINE;",
    'barrel.ts': "export * from './strings';",
    'strings.ts': "export default 'secret.keep default';",
  });
  try { await expect(configure(f.stage, false, parseSync)).rejects.toThrow('cannot resolve the literal export default'); }
  finally { f.close(); }
});

for (const [privateGrants, permissions, expected, column] of [
  ["{ 'https://api.example.com': 'net.fetch https://api.example.com' }", "'secret.keep token'", 'secret.keep token', 21],
  ["'secret.keep private'", "{ 'https://api.example.com': 'net.fetch https://api.example.com' }", 'net.fetch https://api.example.com', 50],
]) test(`aliased grants select only their exported initializer: ${permissions}`, async () => {
  const f = fixture({
    'app.ts': `const origin = 'https://api.example.com';\nconst grants = ${privateGrants};\nconst permissions = ${permissions};\nexport { permissions as grants };`,
  });
  try {
    await configure(f.stage, false, parseSync);
    expect(staticFacts(readFileSync(resolve(f.stage, 'app.ts'), 'utf8'), parseSync).grants).toBe(expected);
    expect(JSON.parse(readFileSync(resolve(f.stage, '__exact_grants.json'), 'utf8'))).toEqual([
      { grant: expected, file: 'app.ts', line: 3, column },
    ]);
  } finally { f.close(); }
});

for (const [specifier, files, expectedFile] of [
  ['./strings.js', { 'strings.ts': "export const LINE = 'secret.keep typescript';", 'strings.js': "export const LINE = 'secret.keep javascript';" }, 'strings.js'],
  ['./strings.mjs', { 'strings.mts': "export const LINE = 'secret.keep typescript';", 'strings.mjs': "export const LINE = 'secret.keep javascript';" }, 'strings.mjs'],
  ['./strings', { 'strings.ts': "export const LINE = 'secret.keep typescript';", 'strings.js': "export const LINE = 'secret.keep javascript';" }, 'strings.ts'],
]) test(`folded imports and Rolldown run the same file: ${specifier}`, async () => {
  const f = fixture({ ...files,
    'app.ts': `import { LINE } from '${specifier}'; export const grants = LINE; export const observed = LINE;`,
  });
  try {
    await configure(f.stage, false, parseSync);
    const build = await rolldown({ ...dataBundle(f.stage), input: resolve(f.stage, 'app.ts'), tsconfig: resolve(f.stage, '__exact_tsconfig.json'), logLevel: 'silent' });
    let code;
    try { code = (await build.generate({ format: 'iife', name: 'app' })).output[0].code; }
    finally { await build.close(); }
    const realm = {}; runInNewContext(code, realm);
    expect(realm.app.grants).toBe(realm.app.observed);
    expect(realm.app.grants).toBe(expectedFile.endsWith('.ts') ? 'secret.keep typescript' : 'secret.keep javascript');
    expect(JSON.parse(readFileSync(resolve(f.stage, '__exact_grants.json'), 'utf8'))).toEqual([
      { grant: realm.app.grants, file: expectedFile, line: 1, column: 21 },
    ]);
  } finally { f.close(); }
});
for (const barrel of [
  "export * from './strings'; export * from './also';",
  "export * from './left'; export * from './right';",
]) test(`star exports resolve the requested binding through ${barrel}`, async () => {
  const f = fixture({ ...constants,
    'app.ts': "import { LINE } from './barrel'; export const grants = LINE;",
    'barrel.ts': barrel, 'also.ts': "export * from './empty';", 'empty.ts': 'export const OTHER = 1;',
    'left.ts': "export { LINE } from './strings';", 'right.ts': "export * from './strings';",
  });
  try {
    await configure(f.stage, false, parseSync);
    expect(staticFacts(readFileSync(resolve(f.stage, 'app.ts'), 'utf8'), parseSync).grants).toBe('secret.keep session.refresh');
  } finally { f.close(); }
});

for (const [barrel, message] of [
  ["export * from './strings'; export * from './other';", 'unambiguously'],
  ["export * as ns from './strings';", 'cannot resolve the literal export LINE'],
]) test(`star exports refuse ${barrel}`, async () => {
  const f = fixture({ ...constants,
    'app.ts': "import { LINE } from './barrel'; export const grants = LINE;",
    'barrel.ts': barrel, 'other.ts': "export const LINE = 'secret.keep other';",
  });
  try { await expect(configure(f.stage, false, parseSync)).rejects.toThrow(message); }
  finally { f.close(); }
});

for (const statement of ["export { grants, observed } from './session';", "export * from './session';",
  "const grants = 'private'; export * from './session';",
  "import { grants as lines, observed } from './session'; export { lines as grants, observed };"]) {
  test(`re-exported grants are frozen and retain their source: ${statement}`, async () => {
    const f = fixture({
      'app.ts': statement,
      'session.ts': "const LINES = ['net.fetch https://api.example.com'];\nLINES.push('secret.keep later');\nexport const grants = LINES.join('\\n');\nexport const observed = LINES.length;\n",
    });
    try {
      await configure(f.stage, false, parseSync);
      const build = await rolldown({ ...dataBundle(f.stage), input: resolve(f.stage, 'app.ts'), tsconfig: resolve(f.stage, '__exact_tsconfig.json'), logLevel: 'silent' });
      let code;
      try { code = (await build.generate({ format: 'iife', name: 'app' })).output[0].code; }
      finally { await build.close(); }
      const realm = {}; runInNewContext(code, realm);
      expect(realm.app.grants).toBe('net.fetch https://api.example.com');
      expect(realm.app.observed).toBe(2);
      expect(staticFacts(readFileSync(resolve(f.stage, 'app.ts'), 'utf8'), parseSync).grants).toBe(realm.app.grants);
      expect(JSON.parse(readFileSync(resolve(f.stage, '__exact_grants.json'), 'utf8'))).toEqual([
        { grant: realm.app.grants, file: 'session.ts', line: 1, column: 16 },
      ]);
    } finally { f.close(); }
  });
}

for (const statement of ["export { grants } from 'a-package';", "export * from 'a-package';"]) test(`grants refuse ${statement}`, async () => {
  const f = fixture({ 'app.ts': statement });
  try { await expect(configure(f.stage, false, parseSync)).rejects.toThrow('cannot use package binding "a-package"'); }
  finally { f.close(); }
});

for (const statement of ["export { grants } from './session';", "export * from './session';"]) test(`re-exported grants must be joined: ${statement}`, async () => {
  const f = fixture({ 'app.ts': statement, 'session.ts': "export const grants = ['secret.keep array'];" });
  try { await expect(configure(f.stage, false, parseSync)).rejects.toThrow('session.ts:1:23: grants must be a string; join the literal array'); }
  finally { f.close(); }
});

test('keyed grants name the selected literals in the receipt', async () => {
  const f = fixture({ 'app.ts': "const origin = 'https://api.example.com';\nexport const grants = {\n  'https://other.example': 'secret.keep other',\n  'https://api.example.com': [\n    'net.fetch https://api.example.com',\n    'secret.keep selected',\n  ],\n};\n" });
  try {
    await configure(f.stage, false, parseSync);
    expect(JSON.parse(readFileSync(resolve(f.stage, '__exact_grants.json'), 'utf8'))).toEqual([
      { grant: 'net.fetch https://api.example.com', file: 'app.ts', line: 5, column: 5 },
      { grant: 'secret.keep selected', file: 'app.ts', line: 6, column: 5 },
    ]);
  } finally { f.close(); }
});

for (const [origin, fetch, socket] of [
  ['http://127.0.0.1:4400', 'http://127.0.0.1:4400', 'ws://127.0.0.1:4400'],
  ['http://localhost:80', 'http://localhost:80', 'ws://localhost:80'],
  ['http://LOCALHOST:4400', 'http://LOCALHOST:4400', 'ws://LOCALHOST:4400'],
  ['https://LOCALHOST:443', 'https://LOCALHOST:443/', 'wss://LOCALHOST:443/'],
  ['http://LOCALHOST:80', 'http://localhost', 'ws://localhost'],
]) test(`a backend override moves imported grants and their receipt entries from ${origin}`, async () => {
  const previous = process.env.EXACT_BACKEND_ORIGIN;
  process.env.EXACT_BACKEND_ORIGIN = 'http://127.0.0.1:4412';
  const f = fixture({
    'app.ts': `import { LINES } from './lines';\nconst origin = '${origin}';\nexport const grants = LINES.join('\\n');`,
    'lines.ts': `export const LINES = [\n  'net.fetch ${fetch}',\n  'net.websocket ${socket}',\n  'secret.keep token',\n];`,
  });
  try {
    await configure(f.stage, false, parseSync);
    expect(staticFacts(readFileSync(resolve(f.stage, 'app.ts'), 'utf8'), parseSync).grants).toBe('net.fetch http://127.0.0.1:4412\nnet.websocket ws://127.0.0.1:4412\nsecret.keep token');
    expect(JSON.parse(readFileSync(resolve(f.stage, '__exact_grants.json'), 'utf8'))).toEqual([
      { grant: 'net.fetch http://127.0.0.1:4412', file: 'lines.ts', line: 2, column: 3 },
      { grant: 'net.websocket ws://127.0.0.1:4412', file: 'lines.ts', line: 3, column: 3 },
      { grant: 'secret.keep token', file: 'lines.ts', line: 4, column: 3 },
    ]);
    expect(JSON.parse(readFileSync(resolve(f.stage, '__exact_backend.json'), 'utf8')).origin).toBe('http://127.0.0.1:4412');
  } finally {
    if (previous === undefined) delete process.env.EXACT_BACKEND_ORIGIN; else process.env.EXACT_BACKEND_ORIGIN = previous;
    f.close();
  }
});
for (const [expression, expected] of [
  ['LINE', 'secret.keep session.refresh'],
  ["PREFIX + REFRESH", 'secret.keep session.refresh'],
  ['`${PREFIX}${REFRESH}`', 'secret.keep session.refresh'],
  ["LINES.join('\\n')", 'secret.keep session.refresh\nsecret.keep session.device'],
  ["['net.fetch https://api.example.com', ...LINES].join('\\n')", 'net.fetch https://api.example.com\nsecret.keep session.refresh\nsecret.keep session.device'],
  ["[...LINES, ...LINES].join('\\n')", 'secret.keep session.refresh\nsecret.keep session.device\nsecret.keep session.refresh\nsecret.keep session.device'],
]) test(`grants fold app-local imports in ${expression}`, async () => {
  const f = fixture({ ...constants, 'app.ts': `import { PREFIX, REFRESH, LINE, LINES } from './barrel';\nexport const grants = ${expression};\n` });
  try {
    await configure(f.stage, false, parseSync);
    expect(staticFacts(readFileSync(resolve(f.stage, 'app.ts'), 'utf8'), parseSync).grants).toBe(expected);
  } finally { f.close(); }
});

test('the static reader resolves captured path mappings and extension substitution', async () => {
  const f = fixture({
    ...constants,
    'app.ts': "import { LINES } from '@/strings.js';\nexport const grants = LINES.join('\\n');\n",
    'tsconfig.json': '{"compilerOptions":{"paths":{"@/*":["./*"]}}}',
  });
  try {
    await configure(f.stage, false, parseSync);
    expect(staticFacts(readFileSync(resolve(f.stage, 'app.ts'), 'utf8'), parseSync).grants).toBe('secret.keep session.refresh\nsecret.keep session.device');
  } finally { f.close(); }
});

test('a bare import grants nothing, and runtime array mutation cannot change the resolved grants', async () => {
  const f = fixture({
    'app.ts': "import { LINES } from './strings';\nimport './unused';\nLINES.push('secret.keep later');\nexport const grants = LINES.join('\\n');\nexport const observed = LINES.length;\n",
    'strings.ts': "export const LINES = ['secret.keep first'];\n",
    'unused.ts': "export const grants = 'secret.keep unused';\n",
  });
  try {
    await configure(f.stage, false, parseSync);
    const build = await rolldown({ ...dataBundle(f.stage), input: resolve(f.stage, 'app.ts'), tsconfig: resolve(f.stage, '__exact_tsconfig.json'), logLevel: 'silent' });
    let code;
    try { code = (await build.generate({ format: 'iife', name: 'app' })).output[0].code; }
    finally { await build.close(); }
    const realm = {};
    runInNewContext(code, realm);
    expect(realm.app.observed).toBe(2);
    expect(realm.app.grants).toBe('secret.keep first');
    expect(JSON.parse(readFileSync(resolve(f.stage, '__exact_grants.json'), 'utf8'))).toEqual([
      { grant: 'secret.keep first', file: 'strings.ts', line: 1, column: 23 },
    ]);
  } finally { f.close(); }
});

for (const statement of [
  "import { LINES } from './session'; export const grants = LINES.join('\\n');",
  "import { LINES } from './session'; import { T, join } from './types'; export const grants = LINES.join('\\n') as T;",
  "import { LINES } from './session'; const permissions = LINES.join('\\n'); export { permissions as grants };",
  "import { grants as permissions, type T } from './session'; export { permissions as grants };",
  "export { grants } from './session';",
  "export { grants, type T } from './session';",
  "export * from './session';",
]) test(`freezing grants preserves module initialization: ${statement}`, async () => {
  const f = fixture({
    'app.ts': `${statement}\nimport { state } from './state';\nexport const answer = () => state.initializations;`,
    'state.ts': 'export const state = { initializations: 0 };',
    'types.ts': "throw new Error('unused imports must remain erased'); export type T = string; export const join = 0;",
    'session.ts': "import { state } from './state';\nstate.initializations++;\nexport const LINES = ['secret.keep session'];\nexport const grants = LINES.join('\\n');\nexport type T = string;",
  });
  try {
    await configure(f.stage, false, parseSync);
    const build = await rolldown({ ...dataBundle(f.stage), input: resolve(f.stage, 'app.ts'), tsconfig: resolve(f.stage, '__exact_tsconfig.json'), logLevel: 'silent' });
    let code;
    try { code = (await build.generate({ format: 'iife', name: 'app' })).output[0].code; }
    finally { await build.close(); }
    const realm = {}; runInNewContext(code, realm);
    expect(realm.app.answer()).toBe(1);
    expect(realm.app.grants).toBe('secret.keep session');
  } finally { f.close(); }
});

for (const wrapper of ['as const', 'satisfies Record<string, string>']) test(`keyed grant locations survive inlined settings and ${wrapper}`, async () => {
  const line = "  'https://api.example.com': 'net.fetch https://api.example.com'";
  const f = fixture({
    'app.ts': `const origin = import.meta.env.API ?? 'https://api.example.com';\nexport const grants = {\n${line}\n} ${wrapper};`,
  }, { env: ['API'] });
  try {
    await configure(f.stage, false, parseSync);
    expect(JSON.parse(readFileSync(resolve(f.stage, '__exact_grants.json'), 'utf8'))).toEqual([
      { grant: 'net.fetch https://api.example.com', file: 'app.ts', line: 3, column: line.indexOf("'net.fetch") + 1 },
    ]);
  } finally { f.close(); }
});

test('grant locations refer to authored syntax after settings are inlined', async () => {
  const code = "const API = import.meta.env.API ?? 'https://built.example'; export const grants = `net.fetch ${API}\nsecret.keep next`;\n";
  const f = fixture({ 'app.ts': code }, { env: ['API'] });
  try {
    await configure(f.stage, false, parseSync);
    expect(JSON.parse(readFileSync(resolve(f.stage, '__exact_grants.json'), 'utf8'))).toEqual([
      { grant: 'net.fetch https://built.example', file: 'app.ts', line: 1, column: code.indexOf('`') + 1 },
      { grant: 'secret.keep next', file: 'app.ts', line: 2, column: 1 },
    ]);
  } finally { f.close(); }
});

for (const file of ['app.ts', 'lines.ts']) test(`mixed escaped and physical newlines retain receipt locations in ${file}`, async () => {
  const f = fixture({
    'app.ts': "export { grants } from './lines';",
    [file]: "export const grants = `secret.keep a\\nsecret.keep b\nsecret.keep c`;",
  });
  try {
    await configure(f.stage, false, parseSync);
    expect(JSON.parse(readFileSync(resolve(f.stage, '__exact_grants.json'), 'utf8'))).toEqual([
      { grant: 'secret.keep a', file, line: 1, column: 23 },
      { grant: 'secret.keep b', file, line: 1, column: 39 },
      { grant: 'secret.keep c', file, line: 2, column: 1 },
    ]);
  } finally { f.close(); }
});

for (const [name, expression, positions] of [
  ['escaped newlines in a string', "'secret.keep a\\nsecret.keep b'", [[1, 23], [1, 39]]],
  ['a continued string', "'secret.keep a\\n\\\nsecret.keep b'", [[1, 23], [2, 1]]],
  ['a continued template', "`secret.keep a\\\r\n\\nsecret.keep b`", [[1, 23], [2, 3]]],
  ['hexadecimal and Unicode LF escapes', "`secret.keep a\\x0asecret.keep b\\u000asecret.keep c\\u{a}secret.keep d`", [[1, 23], [1, 41], [1, 60], [1, 78]]],
  ['CRLF template newlines', '`secret.keep a\r\nsecret.keep b`', [[1, 23], [2, 1]]],
  ['CR template newlines', '`secret.keep a\rsecret.keep b`', [[1, 23], [2, 1]]],
  ['an escaped backslash before a physical newline', '`secret.keep a\\\\n\nsecret.keep b`', [[1, 23], [2, 1]]],
]) test(`grant locations follow ${name}`, () => {
  const facts = staticFacts(`export const grants = ${expression};`, parseSync, { locations: true });
  expect(facts.grantSources).toEqual(facts.grants.split('\n').map((grant, i) => ({
    grant, file: 'app.ts', line: positions[i][0], column: positions[i][1],
  })));
});

for (const source of ['a-package', './node_modules/a-package/index.js']) test(`grants refuse installed package constants through ${source}`, async () => {
  const f = fixture({
    'app.ts': `import { LINE } from '${source}';\nexport const grants = LINE;\n`,
    'node_modules/a-package/package.json': '{"name":"a-package","version":"1.0.0","main":"index.js"}',
    'node_modules/a-package/index.js': "export const LINE = 'secret.keep package';\n",
  });
  try { await expect(configure(f.stage, false, parseSync)).rejects.toThrow(`cannot use package binding "${source}"`); }
  finally { f.close(); }
});

test('grants refuse generated package bindings reached through paths', async () => {
  const f = fixture({
    'app.ts': "import { LINE } from '@native/constants';\nexport const grants = LINE;\n",
    'tsconfig.json': '{"compilerOptions":{"paths":{"@native/constants":["./__exact_modules/constants.ts"]}}}',
    '__exact_modules/constants.ts': "export const LINE = 'secret.keep package';\n",
  });
  try { await expect(configure(f.stage, false, parseSync)).rejects.toThrow('cannot use package binding "@native/constants"'); }
  finally { f.close(); }
});

for (const [name, expression, files, message] of [
  ['package import', "LINE", {}, 'cannot use package binding "a-package"'],
  ['package re-export', "LINE", { 'barrel.ts': "export { LINE } from 'a-package';" }, 'cannot use package binding "a-package"'],
  ['call', 'LINE()', { 'barrel.ts': "export const LINE = () => 'secret.keep called';" }, 'cannot call code'],
  ['conditional', 'LINE', { 'barrel.ts': "export const LINE = true ? 'secret.keep yes' : 'secret.keep no';" }, 'conditional expressions are not allowed'],
  ['computed array', "LINE.join('\\n')", { 'barrel.ts': "export const LINE = ['secret.keep mapped'].map(x => x);" }, 'cannot call code'],
  ['computed element', 'LINE[0]', { 'barrel.ts': "export const LINE = ['secret.keep element'];" }, 'cannot read a computed value or array element'],
  ['array hole', "LINE.join('\\n')", { 'barrel.ts': "export const LINE = [, 'secret.keep hole'];" }, 'cannot contain a hole'],
  ['non-array spread', "[...LINE].join('\\n')", { 'barrel.ts': "export const LINE = 'secret.keep spread';" }, 'may spread only a literal array'],
  ['mutable binding', 'LINE', { 'barrel.ts': "export let LINE = 'secret.keep mutable';" }, 'require a top-level const'],
  ['binding cycle', 'LINE', { 'barrel.ts': "import { OTHER } from './cycle'; export const LINE = OTHER;", 'cycle.ts': "import { LINE } from './barrel'; export const OTHER = LINE;" }, 'contain a constant cycle'],
  ['re-export cycle', 'LINE', { 'barrel.ts': "export { LINE } from './cycle';", 'cycle.ts': "export { LINE } from './barrel';" }, 'cannot resolve the literal export LINE'],
]) test(`grants refuse a ${name} with its source location`, async () => {
  const from = name === 'package import' ? 'a-package' : './barrel';
  const f = fixture({ ...files, 'app.ts': `import { LINE } from '${from}';\nexport const grants = ${expression};\n` });
  try {
    let error;
    try { await configure(f.stage, false, parseSync); } catch (caught) { error = caught; }
    expect(error?.message).toContain(message);
    expect(error?.message).toMatch(/(?:app|barrel|cycle)\.ts:\d+:\d+: grants /);
  } finally { f.close(); }
});

for (const web of [false, true]) for (const dependency of [
  "import { API } from './app';\nexport const LINE = `net.fetch ${API}`;\n",
  "const API = import.meta.env.API ?? 'https://built.example';\nexport const LINE = `net.fetch ${API}`;\n",
]) test(`grants refuse an imported launch-setting dependency on ${web ? 'web' : 'native'}`, async () => {
  const f = fixture({
    'app.ts': "import { LINE } from './barrel';\nexport const API = import.meta.env.API ?? 'https://built.example';\nexport const grants = LINE;\n",
    'barrel.ts': dependency,
  }, { env: [{ name: 'API', launch: true }] });
  try { await expect(configure(f.stage, false, parseSync, web)).rejects.toThrow('depend on launch-marked setting API; grants are fixed at build'); }
  finally { f.close(); }
});

for (const value of ['LINES', "['secret.keep first', ...LINES]"]) test(`keyed grants still refuse ${value}`, async () => {
  const f = fixture({
    ...constants,
    'app.ts': `import { LINES } from './strings';\nconst origin = 'https://api.example.com';\nexport const grants = { 'https://api.example.com': ${value} };\n`,
  });
  try { await expect(configure(f.stage, false, parseSync)).rejects.toThrow('expected a string literal grant line'); }
  finally { f.close(); }
});

test('a reused captured stage restores the authored backend after an override is removed',async()=>{
  const {configure,staticFacts}=await import('../src/typescript.mjs');
  const {parseSync}=await import('rolldown/utils');
  const {mkdtempSync,mkdirSync,writeFileSync,readFileSync,rmSync}=await import('node:fs');
  const {tmpdir}=await import('node:os');const {join}=await import('node:path');
  const root=mkdtempSync(join(tmpdir(),'exact-backend-stage-')),app=join(root,'app'),stage=join(root,'stage');
  const env=process.env.EXACT_BACKEND_ORIGIN; delete process.env.EXACT_BACKEND_ORIGIN;
  try {
    mkdirSync(app);mkdirSync(stage);mkdirSync(join(app,'.exact'));
    const source="const origin='http://127.0.0.1:4400'; export const grants=`net.fetch ${origin}`;";
    writeFileSync(join(stage,'app.ts'),source);writeFileSync(join(stage,'__exact_backend_source.json'),JSON.stringify(source));
    writeFileSync(join(stage,'__exact_paths.json'),JSON.stringify({app,mounts:[]}));
    writeFileSync(join(app,'.exact/backend-origin'),'http://127.0.0.1:4412');
    await configure(stage,true,parseSync);
    expect(readFileSync(join(stage,'app.ts'),'utf8')).toContain('4412');
    const backend = () => JSON.parse(readFileSync(join(stage,'__exact_backend.json'),'utf8'));
    expect(backend()).toEqual({origin:'http://127.0.0.1:4412',warning:expect.stringContaining('overriding')});
    rmSync(join(app,'.exact/backend-origin'));await configure(stage,true,parseSync);
    expect(staticFacts(readFileSync(join(stage,'app.ts'),'utf8'),parseSync).grants).toBe('net.fetch http://127.0.0.1:4400');
    expect(JSON.parse(readFileSync(join(stage,'__exact_backend_source.json'),'utf8'))).toBe(source);
    expect(backend()).toEqual({origin:'http://127.0.0.1:4400',warning:null});
    // An authored edit equal to the previous injection is still a new captured source.
    const edited=source.replace('4400','4412');writeFileSync(join(stage,'__exact_backend_source.json'),JSON.stringify(edited));
    await configure(stage,true,parseSync);
    expect(staticFacts(readFileSync(join(stage,'app.ts'),'utf8'),parseSync).grants).toBe('net.fetch http://127.0.0.1:4412');
    expect(backend()).toEqual({origin:'http://127.0.0.1:4412',warning:null});
  } finally {if(env!==undefined) process.env.EXACT_BACKEND_ORIGIN=env;rmSync(root,{recursive:true,force:true});}
});
