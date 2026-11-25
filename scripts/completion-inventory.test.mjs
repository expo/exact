import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { discover, inventory, markdown, rustFunctions } from './completion-inventory.mjs';

test('every source entry has exactly one ownership inference with evidence', () => {
  // The census reads runner/src, host/apple, host/web, host/web-js,
  // host/web-capabilities, host/web-geometry, host/render/src and host/painted/src.
  const result = inventory();
  assert.deepEqual(result.errors, [], result.errors.join('\n'));
  for (const [host, symbol] of [
    ['runner', 'Runner::fulfill'], ['runner', 'Runner::presence_answer'],
    ['runner', 'Runner::answer_hold'], ['runner', 'Runner::set_shared'],
    ['runner', 'Runner::canvas_image'], ['runner', 'Runner::fire_native_timer'],
    ['runner', 'Runner::append_request#next_ticket:1'],
    ['runner', 'Runner::module_call_linked#next:1'], ['runner', 'Runner::module_call_linked#next:2'],
    ['apple', 'exact_fulfill_surface'], ['web', 'exact_fulfill'],
    ['apple', 'exact_frame_health_record'],
    ['apple', 'exact_widget_reload'],
    ['apple', 'exact_web_answer'], ['apple', 'exact_activity_send'], ['apple', 'exact_native_abi'],
    ['web-js', 'm_end'], ['painted', 'Presenter::pump'],
    ['painted', 'serve'], ['painted', 'handle'], ['painted', 'handle_for_window'],
    ['painted', 'Java_dev_exact_canvas_Native_answer'], ['render', 'settle_aborts'],
  ]) {
    assert.ok(result.entries.some(e => e.host === host && e.symbol === symbol), `${host}:${symbol} missing`);
  }
  const text = markdown(result.entries);
  assert.ok(text.includes('durable when the ticket names'));
}, 60_000);

test('every public Runner receipt entry is enumerated and classified', () => {
  const result = inventory();
  const entries = result.entries.filter(e => e.host === 'runner' &&
    e.symbol.startsWith('Runner::') && (e.returnsReceipt || e.returnsCommitAggregate));
  const listing = entries.map(e => `${e.id}: ${e.family ?? 'UNCLASSIFIED'}`).join('\n');
  assert.ok(entries.some(e => e.symbol === 'Runner::deliver'), listing);
  const door = entries.find(e => e.symbol === 'Runner::deliver');
  assert.ok(door.protocol.includes('subscription'), listing);
  assert.ok(door.protocol.includes('durable operation'), listing);
  for (const [symbol, protocol, kind] of [
    ['Runner::send_turn#next_ticket:1', 'one-shot', 'UI-owned'],
    ['Runner::native_timer_completion', 'subscription', 'UI-owned'],
    ['Runner::canvas_draw_completion', 'subscription', 'UI-owned'],
    ['Runner::canvas_reply', 'subscription', 'UI-owned'],
    ['Runner::send_background#next_ticket:1', 'durable operation', 'durable'],
    ['Runner::take_reports#next_ticket:1', 'durable operation', 'durable'],
    ['Runner::tool_step#next_ticket:1', 'durable operation', 'durable'],
    ['Runner::module_work_landed#next_ticket:1', 'durable operation', 'durable'],
  ]) {
    const entry = result.entries.find(e => e.host === 'runner' && e.symbol === symbol);
    assert.deepEqual(entry?.protocol, [protocol], symbol);
    assert.equal(entry?.class, kind, symbol);
  }
  const binding = result.entries.find(e => e.symbol === 'CompletionReply::subscription');
  assert.ok(binding?.protocol.includes('subscription'), listing);
  for (const entry of entries) {
    assert.ok(entry.family && entry.protocol?.length && entry.lifetime && entry.class && entry.evidence, listing);
  }
  assert.deepEqual(result.errors, [], `${listing}\n${result.errors.join('\n')}`);
}, 60_000);

test('new runner APIs, host exports, adapters and mint sites cannot pass unclassified', () => {
  const root = mkdtempSync(join(tmpdir(), 'exact-completion-inventory-'));
  try {
    for (const dir of ['runner/src', 'host/apple/src', 'host/apple/Sources', 'host/apple/webarm', 'host/apple/modules',
      'host/apple/tests', 'host/web/src', 'host/web-js/motion/src',
      'host/web-capabilities/src', 'host/web-geometry/src', 'host/render/src', 'host/painted/src']) {
      mkdirSync(join(root, dir), { recursive: true });
    }
    const file = join(root, 'runner/src/runner.rs');
    writeFileSync(file, 'impl<D> Runner<D> { pub fn known(&mut self) -> CommitReceipt { self.commit() } }');
    const table = [{ family: 'fixture', lifetime: 'fixture session', class: 'UI-owned', inference: true,
      protocol: ['one-shot'],
      evidence: 'fixture owns its pending completion', ids: ['runner:runner/src/runner.rs:Runner::known'] }];
    assert.deepEqual(inventory(root, table).errors, []);
    writeFileSync(file, readFileSync(file, 'utf8') + `
      impl<D: Source<Vec<Item>>> Runner<D> where D: for<'a> Source<&'a Item> {
        pub const fn new_receipt<F: Fn(u64)>(&self, f: F) -> Result<Option<CommitReceipt>, Error> { todo!() }
        pub fn new_completion(&mut self, outcome: Outcome) {
          self.next_ticket += 1;
          #[cfg(test)] { self.next_ticket += 1; }
        }
        fn other_allocator(&mut self) { self.extensions.next += 1; SERIAL.fetch_add(1, Relaxed); }
        fn unnamed_allocator(&mut self) { self.extensions.arbitrary += 1; }
        fn assigned_allocator(&mut self) { self.extensions.requests = self.extensions.requests + 1; }
        pub fn r#raw_completion(&mut self) -> CommitReceipt { todo!() }
      }
      impl Reply {
        pub fn send(&mut self, outcome: Outcome) {}
        pub fn message(&mut self, message: Message) {}
      }
      impl Native { pub fn changed(&self, topic: &str) {} }
      impl AnotherOwner { pub fn complete(&mut self, result: Outcome) {} }
    `);
    writeFileSync(join(root, 'host/apple/src/extra.rs'), `macro_rules! exports { () => {
      #[unsafe(no_mangle)] pub unsafe extern "C" fn new_apple() {}
      #[export_name = "aliased_export"] extern "C" fn internal_name() {}
      #[no_mangle] pub extern fn implicit_c() {}
    }; }`);
    writeFileSync(join(root, 'runner/src/external.rs'), `macro_rules! shared_exports { () => {
      #[no_mangle] pub extern "C" fn new_external() {}
    }; }`);
    writeFileSync(join(root, 'host/apple/src/imported.rs'), 'exact_runner::shared_exports!();');
    writeFileSync(join(root, 'host/apple/Sources/Extra.swift'), `
      // @_cdecl("comment_only")
      @_cdecl("new_swift_export") public func callback() {}
    `);
    for (const dir of ['webarm', 'modules', 'tests']) {
      writeFileSync(join(root, `host/apple/${dir}/Extra.swift`), `@_cdecl("${dir}_answer") public func callback() {}`);
    }
    writeFileSync(join(root, 'host/web/src/extra.rs'), '#[no_mangle] pub extern "C" fn new_web() {}');
    writeFileSync(join(root, 'host/web-js/motion/src/extra.rs'), '#[no_mangle] pub extern "C" fn new_motion() {}');
    writeFileSync(join(root, 'host/web-js/module.mjs'), 'const leaf = { lib: `#[no_mangle] pub extern "C" fn embedded_export() {}` };');
    writeFileSync(join(root, 'host/web-capabilities/src/extra.rs'), '#[no_mangle] pub extern "C" fn new_capability() {}');
    writeFileSync(join(root, 'host/render/src/extra.rs'), 'fn adapter(runner: &mut Runner<D>) { runner.new_completion(outcome); }');
    writeFileSync(join(root, 'host/painted/src/extra.rs'), `
      impl<D> CanvasHost<D> { pub fn new_reader_answer(&mut self) {} }
      impl<D> Presenter<D> { pub fn new_presenter_answer(&mut self) {} }
      impl<D> Presenter<D> {
        fn runner_adapter(&mut self) { self.host.runner_mut().new_completion(outcome); }
        fn host_adapter(&mut self) { self.host.new_completion(outcome); }
      }
      pub fn new_agent_answer(p: &mut Presenter<D>) { p.pump(0.0); p.answer(); }
      impl Agent { pub fn r#answer(&mut self) {} }
      #[cfg(unix)] pub fn platform_answer() {}
      #[cfg(windows)] pub fn platform_answer() {}
      #[no_mangle] pub unsafe extern "system" fn Java_new_answer() {}
    `);
    const result = inventory(root, table);
    const expected = [
      'runner:runner/src/runner.rs:Runner::new_receipt',
      'runner:runner/src/runner.rs:Runner::new_completion',
      'runner:runner/src/runner.rs:Runner::new_completion#next_ticket:1',
      'runner:runner/src/runner.rs:Runner::other_allocator#next:1',
      'runner:runner/src/runner.rs:Runner::other_allocator#SERIAL.fetch_add:1',
      'runner:runner/src/runner.rs:Runner::unnamed_allocator#arbitrary:1',
      'runner:runner/src/runner.rs:Runner::assigned_allocator#requests:1',
      'runner:runner/src/runner.rs:Runner::raw_completion',
      'runner:runner/src/runner.rs:Reply::send',
      'runner:runner/src/runner.rs:Reply::message',
      'runner:runner/src/runner.rs:Native::changed',
      'runner:runner/src/runner.rs:AnotherOwner::complete',
      'apple:host/apple/src/extra.rs:new_apple',
      'apple:host/apple/src/extra.rs:aliased_export',
      'apple:host/apple/src/extra.rs:implicit_c',
      'apple:runner/src/external.rs:new_external',
      'runner:runner/src/external.rs:new_external',
      'apple:host/apple/Sources/Extra.swift:new_swift_export',
      'apple:host/apple/webarm/Extra.swift:webarm_answer',
      'apple:host/apple/modules/Extra.swift:modules_answer',
      'web:host/web/src/extra.rs:new_web',
      'web-js:host/web-js/motion/src/extra.rs:new_motion',
      'web-js:host/web-js/module.mjs:embedded_export',
      'web-capabilities:host/web-capabilities/src/extra.rs:new_capability',
      'render:host/render/src/extra.rs:adapter',
      'painted:host/painted/src/extra.rs:CanvasHost::new_reader_answer',
      'painted:host/painted/src/extra.rs:Presenter::new_presenter_answer',
      'painted:host/painted/src/extra.rs:Presenter::runner_adapter',
      'painted:host/painted/src/extra.rs:Presenter::host_adapter',
      'painted:host/painted/src/extra.rs:Java_new_answer',
      'painted:host/painted/src/extra.rs:new_agent_answer',
      'painted:host/painted/src/extra.rs:Agent::answer',
      'painted:host/painted/src/extra.rs:platform_answer',
      'painted:host/painted/src/extra.rs:platform_answer#definition:2',
    ];
    assert.deepEqual(result.entries.filter(e => !e.evidence).map(e => e.id).sort(), expected.sort());
    assert.equal(result.errors.length, expected.length);
    assert.ok(result.errors.every(e => e.startsWith('unclassified:')));
    assert.equal(result.entries.find(e => e.symbol === 'Runner::new_receipt').returnsReceipt, true);
    assert.ok(inventory(root, [...table, ...table]).errors.some(e => e.startsWith('classified twice:')));
    assert.ok(inventory(root, [{ ...table[0], ids: ['gone'] }]).errors.some(e => e === 'stale classification: gone'));
    assert.ok(inventory(root, [{ ...table[0], inference: false }]).errors.some(e => e.startsWith('incomplete classification:')));
    for (const protocol of [undefined, [], ['unknown'], ['one-shot', 'one-shot'], 'one-shot']) {
      assert.ok(inventory(root, [{ ...table[0], protocol }]).errors.some(e => e.startsWith('incomplete classification:')),
        `invalid protocol accepted: ${JSON.stringify(protocol)}`);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('protocols distinguish replies, subscriptions and durable operations in both outputs', () => {
  const result = inventory();
  for (const [symbol, protocol] of [
    ['Runner::fulfill', ['one-shot', 'subscription', 'durable operation']],
    ['Runner::changed', ['subscription']],
    ['Runner::send_background#next_ticket:1', ['durable operation']],
  ]) {
    const entry = result.entries.find(e => e.symbol === symbol);
    assert.deepEqual(entry.protocol, protocol, symbol);
    assert.deepEqual(JSON.parse(JSON.stringify(entry)).protocol, protocol);
    const text = markdown([entry]);
    assert.ok(text.includes('| Family | runner | Total |'));
    assert.ok(text.includes(`| ${entry.family} | 1 | 1 |`));
    assert.ok(text.includes('| Protocol |'));
    assert.ok(text.includes(`| ${protocol.join(', ')} |`));
  }
}, 60_000);

test('public reply and announcement APIs outside Runner are classified', () => {
  const entries = inventory().entries;
  for (const [symbol, family, protocol] of [
    ['Reply::send', 'replies', ['one-shot', 'subscription', 'durable operation']],
    ['Reply::message', 'replies', ['one-shot', 'subscription', 'durable operation']],
    ['Native::changed', 'module announcements', ['subscription']],
  ]) {
    const entry = entries.find(e => e.host === 'runner' && e.symbol === symbol);
    assert.ok(entry, symbol);
    assert.equal(entry.family, family);
    assert.deepEqual(entry.protocol, protocol);
  }
}, 60_000);

test('painted picker and share adapters cannot bypass the inventory', () => {
  const entries = inventory().entries;
  for (const [symbol, family] of [
    ['Presenter::finish_files', 'picker results'],
    ['Presenter::receive', 'shared tokens'],
    ['Presenter::land', 'shared tokens'],
  ]) {
    const entry = entries.find(e => e.host === 'painted' && e.symbol === symbol);
    assert.ok(entry, symbol);
    assert.equal(entry.family, family);
  }
}, 60_000);

test('painted pumps deliver replies and host-timed runner dispatch delivers input', () => {
  const entries = inventory().entries;
  for (const [host, symbol, family] of [
    ['painted', 'Presenter::pump', 'replies'],
    ['painted', 'Presenter::pump_with_clock', 'replies'],
    ['painted', 'Host::fulfill_ready', 'replies'],
    ['painted', 'Presenter::release_held_work', 'request lifecycle'],
    ['runner', 'Runner::dispatch_at', 'UI input'],
    ['runner', 'Runner::dispatch_at_with_receipt', 'UI input'],
  ]) {
    assert.equal(entries.find(e => e.host === host && e.symbol === symbol)?.family, family, symbol);
  }
}, 60_000);

test('the generated Markdown wasm exports are enumerated at their template source lines', () => {
  const entries = inventory().entries;
  for (const symbol of ['alloc', 'pieces', 'output']) {
    const entry = entries.find(e => e.file === 'host/web-js/module.mjs' && e.symbol === symbol);
    assert.ok(entry, symbol);
    assert.equal(entry.family, 'presentation helpers');
    assert.equal(entry.kind, 'C ABI export');
    assert.match(readFileSync(entry.file, 'utf8').split('\n')[entry.line - 1], new RegExp(`fn ${symbol}\\(`));
  }
}, 60_000);

test('native send and payment attempt allocators are enumerated', () => {
  const entries = discover();
  for (const [file, symbol] of [
    ['letgo', 'Runner::number_data_call_linked#requests:1'],
    ['bound', 'Runner::forward_body#requests:1'],
    ['sheet', 'Runner::sheet_opened#calls:1'],
    ['sheet', 'Runner::sheet_call#calls:1'],
  ]) {
    assert.ok(entries.some(e => e.file === `runner/src/runner/modules/${file}.rs` && e.symbol === symbol), symbol);
  }
}, 60_000);

test('Android background timer delivery belongs to the current reader', () => {
  const entries = inventory().entries;
  for (const symbol of ['CanvasHost::background', 'Java_dev_exact_canvas_Native_background']) {
    const e = entries.find(e => e.symbol === symbol);
    assert.equal(e.family, 'timer fires');
    assert.equal(e.class, 'UI-owned');
    assert.match(e.lifetime, /reader|session/);
    assert.match(e.evidence, /canvas\/timers\.rs/);
  }
}, 60_000);

test('raw runner identifiers retain their normalized API name and receipt', () => {
  const { functions } = rustFunctions('runner/src/fixture.rs', `
    impl<D> r#Runner<D> { pub fn r#new_completion(&mut self) -> CommitReceipt { todo!() } }
  `);
  assert.equal(functions.length, 1);
  assert.equal(functions[0].owner, 'Runner');
  assert.equal(functions[0].name, 'new_completion');
  assert.equal(functions[0].public, true);
  assert.match(functions[0].returnType, /CommitReceipt/);
});

test('extern without an ABI string exports the default C ABI', () => {
  const { functions } = rustFunctions('host/apple/src/fixture.rs', `
    #[no_mangle] pub extern fn new_completion() {}
    #[unsafe(export_name = "implicit_alias")] pub unsafe extern fn r#aliased() {}
    #[no_mangle] pub extern "Rust" fn rust_abi() {}
  `);
  assert.deepEqual(functions.map(f => f.cExport), [true, true, false]);
  assert.equal(functions[1].exportName, 'implicit_alias');
});

test('the lexer excludes strings, nested comments and test items without hiding following APIs', () => {
  const code = `
    // pub fn fake() -> CommitReceipt {}
    /* outer /* pub fn fake() {} */ still a comment */
    const RAW: &str = br##"pub fn fake() {}"##;
    const NORMAL: &str = "http://example/ /* fake */";
    const CHAR: char = '{';
    struct State { #[cfg(test)] calls: usize, value: u64 }
    #[cfg(test)] use crate::{Fake, Other};
    impl<D> Runner<D> {
      #[cfg(test)] pub fn test_only() {}
      pub(crate) fn internal() {}
      pub fn real<'a>(&'a self) -> CommitReceipt { todo!() }
    }
    #[cfg(test)] mod nested { impl<D> Runner<D> { pub fn fake() {} } }
    impl<D> Runner<D> { pub fn after() {} }
  `;
  const f = rustFunctions('runner/src/fixture.rs', code).functions;
  assert.deepEqual(f.filter(f => f.public).map(f => f.name), ['real', 'after']);
  assert.equal(rustFunctions('runner/src/fixture_tests.rs', code).functions.length, 0);
  assert.equal(rustFunctions('runner/src/tests/fixture.rs', code).functions.length, 0);
  assert.throws(() => rustFunctions('host/apple/src/new.rs', `
    macro_rules! new_export { ($name:ident) => { #[no_mangle] pub extern "C" fn $name() {} }; }
  `), /computed API name/);
  assert.throws(() => rustFunctions('runner/src/broken.rs', 'impl Runner<D> {'), /unclosed/);
});
