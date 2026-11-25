// How `build.mjs --test` runs the Swift host tests once they are built:
// on this Mac, a test class to an `xctest` process; on a simulator, the
// `.xctestrun` xcodebuild wrote, without the Thread Performance Checker.
import { spawn, spawnSync } from './toolchain.mjs';
import { cpSync, existsSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { availableParallelism, homedir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { appleAssets, appleAssetCatalogInventory, rasterIconAssets, iosAssets, plistFile } from './assets.mjs';
import { simulators } from './devices.mjs';
import { encodePng } from '../../scripts/png.mjs';

const read = (cmd, args, opts = {}) => spawnSync(cmd, args, { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, ...opts });
/** The device set xcodebuild makes the simulator clones for parallel test runners in. */
const TESTING_DEVICES = resolve(homedir(), 'Library/Developer/XCTestDevices');

/** The package-screen test loads a package and its app module through their usual separate libraries. */
export async function packageViewFixture(root, fixtureDir, triple, appId, env, runApple) {
  const { packageCompiler, composedModule, moduleLibraryLinkArgs, moduleSeam } = await import('./compose.mjs');
  const dir = resolve(fixtureDir, 'package-screen'), fixtures = resolve(root, 'host/apple/tests/fixtures');
  const tags = ['probe-table', 'probe-plain', 'probe-cleared'];
  const packages = [{ extension: 'screen-probe', module: 'ScreenProbe', className: 'ScreenPackage', views: tags }];
  packageCompiler({ packages, packageDir: dir, packagesRoot: dir, packageTargets: new Map([[dir, ['iphonesimulator', ['-target', triple]]]]),
    seam: resolve(root, 'host/apple/modules/ExactPackage.swift'), packageCopies: [[resolve(fixtures, 'ScreenPackage.swift'), resolve(fixtures, 'AppearanceTable.swift')]],
    ios: true, swiftc: read('xcrun', ['swiftc', '--version']).stdout }).place(dir);
  const composition = resolve(dir, 'ExactComposed.swift');
  writeFileSync(composition, composedModule(packages, true));
  env.TEST_RUNNER_EXACT_PACKAGE_SCREEN_MODULE = resolve(dir, 'libexact_modules.dylib');
  runApple('xcrun', ['--sdk', 'iphonesimulator', 'swiftc', '-target', triple, '-parse-as-library', '-emit-library', '-swift-version', '5',
    '-I', dir, resolve(dir, 'ExactPackage.o'), ...moduleSeam(resolve, root), resolve(fixtureDir, 'ExactDataKeys.swift'), composition,
    resolve(fixtures, 'ScreenModule.swift'), resolve(fixtures, 'AppearanceTable.swift'), ...moduleLibraryLinkArgs(), '-o', env.TEST_RUNNER_EXACT_PACKAGE_SCREEN_MODULE]);
  const name = '@fixture/screen-probe', pkg = resolve(dir, 'node_modules', name);
  mkdirSync(pkg, { recursive: true });
  writeFileSync(resolve(pkg, 'exact.json'), JSON.stringify({ name, version: '1.0.0', kind: 'interface', names: [], views: Object.fromEntries(tags.map(tag => [tag, {}])) }));
  writeFileSync(resolve(pkg, 'package.json'), JSON.stringify({ name, version: '1.0.0', exports: './exact.json' }));
  writeFileSync(resolve(dir, 'package.json'), JSON.stringify({ dependencies: { [name]: '1.0.0' } }));
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ app: { id: appId, name: 'Package screen' }, modules: ['app-table', 'reusable-table', 'clearing-table', 'reusable-cleared'] }));
  for (const [file, key] of [['package-screen', 'PACKAGE_SCREEN'], ['package-screen-split', 'PACKAGE_SCREEN_SPLIT']]) {
    cpSync(resolve(fixtures, `${file}.contract`), resolve(dir, `${file}.contract`));
    const plan = env[`TEST_RUNNER_EXACT_${key}_PLAN`] = resolve(dir, `${file}.plan`);
    runApple(env.EXACT_CONTRACT, ['build', resolve(dir, `${file}.contract`), '-o', plan], { env });
  }
}

/** Rebuild fixture plans with `env.EXACT_CONTRACT`; write their paths to `<prefix>_<KEY>_PLAN`.
 * Plans embed the current schema digest. App entries need the test app's `appId` to boot. */
export function compileFixtures(root, run, env, dir, prefix, appId) {
  const contract = args => run(env.EXACT_CONTRACT, ['build', ...args], { env });
  const watchSync = resolve(dir, 'watch-sync');
  mkdirSync(watchSync, { recursive: true });
  writeFileSync(resolve(watchSync, 'app.json'), JSON.stringify({ app: { id: appId, name: 'Watch sync' } }));
  writeFileSync(resolve(watchSync, 'app.contract'), readFileSync(resolve(root, 'scripts/fixtures/store-refresh.contract')));
  contract([resolve(watchSync, 'app.contract'), '-o', env[`${prefix}_WATCH_SYNC_PLAN`] = resolve(watchSync, 'app.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/layout-only-row.contract'), '-o', env[`${prefix}_LAYOUT_ONLY_ROW_PLAN`] = resolve(dir, 'layout-only-row.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/layout-only-paint.contract'), '-o', env[`${prefix}_LAYOUT_ONLY_PAINT_PLAN`] = resolve(dir, 'layout-only-paint.plan')]);
  // Made afresh: a target from before the tamber->forma rename keeps an app.tamber here, which contract refuses.
  const iconApp = resolve(dir, 'entries/raster-icons');
  rmSync(iconApp, { recursive: true, force: true });
  mkdirSync(resolve(iconApp, 'assets'), { recursive: true });
  writeFileSync(resolve(iconApp, 'app.json'), JSON.stringify({ app: { id: appId, name: 'Raster icons' }, host: { ios: {}, macos: {} } }));
  writeFileSync(resolve(iconApp, 'app.forma'), 'forma icons\n\nicons\n  icon.two = icon(svg "two.svg")\n');
  writeFileSync(resolve(iconApp, 'assets/two.svg'), '<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 24 24"><path fill="#ff0000" d="M0 0h12v24H0Z"/><path fill="#0000ff" d="M12 0h12v24H12Z"/></svg>');
  writeFileSync(resolve(iconApp, 'app.contract'), 'component App\n  view\n    column\n      button\n        image "symbol:icon.two" -exact-symbol-rendering="multicolor"\n        text "Colours"\n      image "symbol:icon.two" -exact-symbol-rendering="multicolor" width=80 testId="raster-wide"\n');
  contract([resolve(iconApp, 'app.contract'), '-o', env[`${prefix}_RASTER_ICONS_PLAN`] = resolve(dir, 'raster-icons.plan')]);
  const iconBundle = env[`${prefix}_RASTER_ICONS_BUNDLE`] = resolve(dir, 'RasterIcons.bundle');
  mkdirSync(iconBundle, { recursive: true });
  writeFileSync(resolve(iconBundle, 'Info.plist'), plistFile({ CFBundleIdentifier: 'dev.exact.raster-icons', CFBundlePackageType: 'BNDL' }));
  appleAssets({ dir: iconApp, name: 'Raster icons', manifest: {} }, iconBundle,
    { platform: prefix === 'EXACT' ? 'macos' : 'ios', icons: [{ svg: 'two.svg', viewBox: '0 0 24 24' }] });

  contract([resolve(root, 'host/apple/tests/fixtures/fragment.contract'), '-o', env[`${prefix}_FRAGMENT_PLAN`] = resolve(dir, 'fragment.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/table-access.contract'), '-o', env[`${prefix}_TABLE_ACCESS_PLAN`] = resolve(dir, 'table-access.plan')]);
  contract([resolve(root, 'scripts/fixtures/prose-list.contract'), '-o', env[`${prefix}_PROSE_LIST_PLAN`] = resolve(dir, 'prose-list.plan')]);
  if (prefix === 'TEST_RUNNER_EXACT') {
    // LaunchColorIOSTests reads the build's XML and compiled catalogue as an app bundle.
    const fixtures = env[`${prefix}_LAUNCH_COLOR_FIXTURES`] = resolve(dir, 'launch-colors');
    for (const [name, manifest] of [
      ['white', { background_color: '#ffffff' }],
      ['opaque', { background_color: '#abc', color_scheme_dark: { background_color: '#123456' } }],
      ['translucent', { background_color: '#abc', color_scheme_dark: { background_color: '#12345680' } }],
      ['p3', { color_scheme_light: { background_color: 'color(display-p3 1 0 0)' }, color_scheme_dark: { background_color: 'color(display-p3 0.1 0.2 0.3 / 0.5)' } }],
    ]) {
      const bundle = resolve(fixtures, `${name}.bundle`);
      mkdirSync(bundle, { recursive: true });
      const keys = iosAssets({ dir, name, manifest }, bundle, false);
      writeFileSync(resolve(bundle, 'Info.plist'), plistFile({ CFBundleIdentifier: `dev.exact.launch.${name}`, CFBundlePackageType: 'BNDL', ...keys }));
    }
  }
  contract([resolve(root, 'host/apple/tests/fixtures/navigation-scroller.contract'), '-o', env[`${prefix}_NAVIGATION_SCROLLER_PLAN`] = resolve(dir, 'navigation-scroller.plan')]);
  const scrollEdges = resolve(root, 'host/apple/tests/fixtures/route-scroll-edges.contract');
  const coverScrollEdges = resolve(dir, 'cover-scroll-edges.contract');
  writeFileSync(coverScrollEdges, readFileSync(scrollEdges, 'utf8').replace('main navigationKey=', 'main viewport-fit="cover" navigationKey='));
  contract([coverScrollEdges, '-o', env[`${prefix}_COVER_SCROLL_EDGES_PLAN`] = resolve(dir, 'cover-scroll-edges.plan')]);
  contract([scrollEdges, '-o', env[`${prefix}_ROUTE_SCROLL_EDGES_PLAN`] = resolve(dir, 'route-scroll-edges.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/bottom-toolbar.contract'), '-o', env[`${prefix}_BOTTOM_TOOLBAR_PLAN`] = resolve(dir, 'bottom-toolbar.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/bottom-toolbar-tabs.contract'), '-o', env[`${prefix}_BOTTOM_TOOLBAR_TABS_PLAN`] = resolve(dir, 'bottom-toolbar-tabs.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/bottom-toolbar-live.contract'), '-o', env[`${prefix}_BOTTOM_TOOLBAR_LIVE_PLAN`] = resolve(dir, 'bottom-toolbar-live.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/overflowing-tabs.contract'), '-o', env[`${prefix}_OVERFLOWING_TABS_PLAN`] = resolve(dir, 'overflowing-tabs.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/shop-scroll.contract'), '-o', env[`${prefix}_SHOP_SCROLL_PLAN`] = resolve(dir, 'shop-scroll.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/quiet-scroll.contract'), '-o', env[`${prefix}_QUIET_SCROLL_PLAN`] = resolve(dir, 'quiet-scroll.plan')]);
  const filteredFeed = resolve(dir, 'filtered-feed.contract');
  writeFileSync(filteredFeed, readFileSync(resolve(root, 'host/apple/tests/fixtures/quiet-scroll.contract'), 'utf8')
    .replace('for n in items ', 'for n in items.filter(n => n % 2 != 0) '));
  contract([filteredFeed, '-o', env[`${prefix}_FILTERED_FEED_PLAN`] = resolve(dir, 'filtered-feed.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/quiet-edge.contract'), '-o', env[`${prefix}_QUIET_EDGE_PLAN`] = resolve(dir, 'quiet-edge.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/scroll-facts.contract'), '-o', env[`${prefix}_SCROLL_FACTS_PLAN`] = resolve(dir, 'scroll-facts.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/sheet-scroll.contract'), '-o', env[`${prefix}_SHEET_SCROLL_PLAN`] = resolve(dir, 'sheet-scroll.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/scroll-snap.contract'), '-o', env[`${prefix}_SCROLL_SNAP_PLAN`] = resolve(dir, 'scroll-snap.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/fixed-page.contract'), '-o', env[`${prefix}_FIXED_PAGE_PLAN`] = resolve(dir, 'fixed-page.plan')]);
  contract([resolve(root, 'scripts/fixtures/collection-fixed-chat.contract'), '-o', env[`${prefix}_COLLECTION_FIXED_PLAN`] = resolve(dir, 'collection-fixed.plan')]);
  const eagerList = resolve(dir, 'collection-fixed-eager.contract');
  writeFileSync(eagerList, readFileSync(resolve(root, 'scripts/fixtures/collection-fixed-chat.contract'), 'utf8')
    .replace('virtualized=true', 'virtualized=false').replace(' estimated-item-height=40', '').replace(' scrollFollowEnd=true', ''));
  contract([eagerList, '-o', env[`${prefix}_COLLECTION_FIXED_EAGER_PLAN`] = resolve(dir, 'collection-fixed-eager.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/side-reach.contract'), '-o', env[`${prefix}_SIDE_REACH_PLAN`] = resolve(dir, 'side-reach.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/arrangement-sizing.contract'), '-o', env[`${prefix}_ARRANGEMENT_SIZING_PLAN`] = resolve(dir, 'arrangement-sizing.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/canvas-capture.contract'), '-o', env[`${prefix}_CANVAS_CAPTURE_PLAN`] = resolve(dir, 'canvas-capture.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/range-adjust.contract'), '-o', env[`${prefix}_RANGE_ADJUST_PLAN`] = resolve(dir, 'range-adjust.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/replacement-hold.contract'), '-o', env[`${prefix}_REPLACEMENT_HOLD_PLAN`] = resolve(dir, 'replacement-hold.plan')]);
  const replaced = resolve(dir, 'replacement-hold-after.contract');
  writeFileSync(replaced, readFileSync(resolve(root, 'host/apple/tests/fixtures/replacement-hold.contract'), 'utf8')
    .replace('"before"', '"after"').replace('      DraftNote()', '      OtherNote()\n      DraftNote()'));
  contract([replaced, '-o', env[`${prefix}_REPLACEMENT_HOLD_AFTER_PLAN`] = resolve(dir, 'replacement-hold-after.plan')]);
  // A test that writes its own manifest names the app the tests link (`SheetFixture`'s `data`).
  env[`${prefix}_TEST_APP_ID`] = appId;
  contract([resolve(root, 'host/apple/tests/fixtures/page-freeze.contract'), '-o', env[`${prefix}_PAGE_FREEZE_PLAN`] = resolve(dir, 'page-freeze.plan')]);
  const clicks = resolve(dir, 'page-freeze-sound');
  mkdirSync(resolve(clicks, 'assets'), { recursive: true });
  const wave = Buffer.alloc(44 + 8820);
  wave.write('RIFF'); wave.writeUInt32LE(wave.length - 8, 4); wave.write('WAVEfmt ', 8); wave.writeUInt32LE(16, 16);
  wave.writeUInt16LE(1, 20); wave.writeUInt16LE(1, 22); wave.writeUInt32LE(44100, 24); wave.writeUInt32LE(88200, 28);
  wave.writeUInt16LE(2, 32); wave.writeUInt16LE(16, 34); wave.write('data', 36); wave.writeUInt32LE(8820, 40);
  writeFileSync(resolve(clicks, 'assets/click.wav'), wave);
  const pulse = readFileSync(resolve(root, 'host/apple/tests/fixtures/page-freeze.contract'), 'utf8')
    .replace('beats=4', 'beats=1').replace('steps=[]', 'steps=[Step(beat=0, src="assets/click.wav", gain=1, group="pulse", note=-1, velocity=100, channel=0, duration=0.1, haptic="")]')
    .replace('  view\n', '  action stop\n    playing = false\n  action stopWithSound\n    playing = false\n    playSound("assets/click.wav")\n  view\n')
    + '      button "Stop" testId="stop" press=stop\n      button "Stop with sound" testId="stop-with-sound" press=stopWithSound\n';
  writeFileSync(resolve(clicks, 'app.contract'), 'sound "assets/click.wav"\n' + pulse);
  contract([resolve(clicks, 'app.contract'), '-o', env[`${prefix}_PAGE_FREEZE_SOUND_PLAN`] = resolve(dir, 'page-freeze-sound.plan')]);
  writeFileSync(resolve(clicks, 'no-sequence.contract'), ('sound "assets/click.wav"\n' + pulse)
    .replace(/  sequence pulse when playing\n    play\([^\n]+\n/, ''));
  contract([resolve(clicks, 'no-sequence.contract'), '-o', env[`${prefix}_PAGE_FREEZE_NO_SEQUENCE_PLAN`] = resolve(dir, 'page-freeze-no-sequence.plan')]);
  // A box's length read against the document's environment stops a test (LLP 1123 D9).
  env[`${prefix}_STRICT_LOCAL_INSETS`] = '1';
  contract([resolve(root, 'host/apple/tests/fixtures/lazy-tab-title.contract'), '-o', env[`${prefix}_LAZY_TAB_TITLE_PLAN`] = resolve(dir, 'lazy-tab-title.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/headless-reply.contract'), '-o', env[`${prefix}_HEADLESS_REPLY_PLAN`] = resolve(dir, 'headless-reply.plan')]);
  for (const [name, key] of [['saved-scheme', 'SAVED_SCHEME'], ['grouped-card', 'GROUPED_CARD'], ['frame-health', 'FRAME_HEALTH'], ['store-refresh', 'STORE_REFRESH'], ['headless', 'HEADLESS'], ['headless-markdown', 'HEADLESS_MARKDOWN'], ['headless-hidden-markdown', 'HEADLESS_HIDDEN_MARKDOWN'], ['headless-live', 'HEADLESS_LIVE'], ['headless-back', 'HEADLESS_BACK'], ['native-navigation-scroll', 'NATIVE_SCROLL'], ['intersect', 'INTERSECT'], ['native-buttons', 'BUTTONS'], ['keyboard-form', 'KEYBOARD'], ['tab-edge', 'TAB_EDGE'], ['repaint', 'REPAINT'], ['still-title', 'STILL_TITLE'], ['navigation-bars', 'NAVIGATION_BARS'], ['large-title', 'LARGE_TITLE'], ['transparent-bar', 'TRANSPARENT_BAR'], ['bar-band', 'BAR_BAND'], ['pad-bar', 'PAD_BAR'], ['pushed-title', 'PUSHED_TITLE'], ['inline-frame', 'INLINE_FRAME'], ['fieldless', 'FIELDLESS'], ['web-documents', 'WEB_DOCUMENTS'], ['iframe-post-messages', 'IFRAME_POST_MESSAGES'], ['iframe-partition', 'IFRAME_PARTITION'], ['iframe-windows', 'IFRAME_WINDOWS'], ['iframe-pdf', 'IFRAME_PDF'], ['iframe-resident-popover', 'IFRAME_RESIDENT_POPOVER'], ['tab-owner', 'TAB_OWNER'], ['grouped-list-reveal', 'GROUPED_REVEAL'], ['due-now', 'DUE_NOW'], ['caret-reveal', 'CARET_REVEAL'], ['cover-scroll', 'COVER_SCROLL'], ['page-edges', 'PAGE_EDGES'], ['barless-route', 'BARLESS_ROUTE'], ['page-edges-grouped', 'PAGE_EDGES_GROUPED'], ['control-bars', 'CONTROL_BARS'], ['cover-tabs', 'COVER_TABS'], ['root-tabs-order', 'ROOT_TABS_ORDER'], ['layout-insert', 'LAYOUT_INSERT'], ['search-scope', 'SEARCH_SCOPE'], ['barless-tab', 'BARLESS_TAB'], ['pull-refresh', 'PULL_REFRESH'], ['edge-effect', 'EDGE_EFFECT'], ['sticky-bar', 'STICKY_BAR'], ['glass-press', 'GLASS_PRESS'], ['surface-pairs', 'SURFACE_PAIRS'], ['prevent-focus', 'PREVENT_FOCUS'], ['accent-bars', 'ACCENT_BARS'], ['accent-tabs', 'ACCENT_TABS'], ['search-tabs', 'SEARCH_TABS'], ['beside-routes', 'BESIDE_ROUTES'], ['bar-insets', 'BAR_INSETS'], ['bar-hero', 'BAR_HERO'], ['bar-control', 'BAR_CONTROL'], ['tips', 'TIPS'], ['boot-control-top', 'BOOT_CONTROL_TOP'], ['boot-control-bottom', 'BOOT_CONTROL_BOTTOM'], ['boot-control-tab', 'BOOT_CONTROL_TAB'], ['boot-dialog', 'BOOT_DIALOG'], ['boot-tabs', 'BOOT_TABS'], ['boot-clipboard', 'BOOT_CLIPBOARD'], ['boot-motion', 'BOOT_MOTION'], ['boot-plain', 'BOOT_PLAIN'], ['boot-padded', 'BOOT_PADDED'], ['boot-transform', 'BOOT_TRANSFORM'], ['boot-tip', 'BOOT_TIP'], ['split-custom', 'SPLIT_CUSTOM'], ['split-tabs', 'SPLIT_TABS'], ['split-back', 'SPLIT_BACK'], ['split-boot', 'SPLIT_BOOT'], ['split-change', 'SPLIT_CHANGE'], ['split-late', 'SPLIT_LATE']]) {
    contract([resolve(root, `scripts/fixtures/${name}.contract`), '-o', env[`${prefix}_${key}_PLAN`] = resolve(dir, `${name}.plan`)]);
  }
  // NamedWindowsTests' links that name a window (LLP 1030.004).
  contract([resolve(root, 'host/apple/tests/fixtures/named-windows.contract'), '-o', env[`${prefix}_NAMED_WINDOWS_PLAN`] = resolve(dir, 'named-windows.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/sticky-transition.contract'), '-o', env[`${prefix}_STICKY_TRANSITION_PLAN`] = resolve(dir, 'sticky-transition.plan')]);
  // DiscreteSwipeTests': `swipe` on a bar with a field and inside an inert node.
  contract([resolve(root, 'host/apple/tests/fixtures/discrete-swipe.contract'), '-o', env[`${prefix}_DISCRETE_SWIPE_PLAN`] = resolve(dir, 'discrete-swipe.plan')]);
  // SwipeScrollDragTests': swipeleft and swiperight rows and a scroller's drag events.
  contract([resolve(root, 'host/apple/tests/fixtures/swipe-scroll-drag.contract'), '-o', env[`${prefix}_SWIPE_SCROLL_DRAG_PLAN`] = resolve(dir, 'swipe-scroll-drag.plan')]);
  // SwipeRoutesIOSTests: swipe rows in a route that pushes another.
  contract([resolve(root, 'host/apple/tests/fixtures/swipe-push.contract'), '-o', env[`${prefix}_SWIPE_PUSH_PLAN`] = resolve(dir, 'swipe-push.plan')]);
  // SnapshotDriverTests' live tree: a bar and a ring under an ancestor's accent.
  contract([resolve(root, 'host/apple/tests/fixtures/live-accent.contract'), '-o', env[`${prefix}_LIVE_ACCENT_PLAN`] = resolve(dir, 'live-accent.plan')]);
  // HeldStateTests' own (a card that presses over a flat leaf), beside them.
  contract([resolve(root, 'host/apple/tests/fixtures/held-state.contract'), '-o', env[`${prefix}_HELD_STATE_PLAN`] = resolve(dir, 'held-state.plan')]);
  // ScrollTimelineTests': a scroll timeline's and a view timeline's consumers (LLP 1057.003 D5).
  contract([resolve(root, 'host/apple/tests/fixtures/scroll-timelines.contract'), '-o', env[`${prefix}_SCROLL_TIMELINES_PLAN`] = resolve(dir, 'scroll-timelines.plan')]);
  // AgentStickyReveal{Mac,IOS}Tests': rows under a sticky header and dock.
  contract([resolve(root, 'host/apple/tests/fixtures/sticky-tap.contract'), '-o', env[`${prefix}_STICKY_TAP_PLAN`] = resolve(dir, 'sticky-tap.plan')]);
  // GroupedCustomHeadersIOSTests': section headers of an avatar beside a name.
  contract([resolve(root, 'host/web-js/conformance/grouped-settings.contract'), '-o', env[`${prefix}_GROUPED_SETTINGS_PLAN`] = resolve(dir, 'grouped-settings.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/grouped-header.contract'), '-o', env[`${prefix}_GROUPED_HEADER_PLAN`] = resolve(dir, 'grouped-header.plan')]);
  // RestartListIOSTests': a restart over a route whose bar covers a virtualized list.
  contract([resolve(root, 'host/apple/tests/fixtures/restart-list.contract'), '-o', env[`${prefix}_RESTART_LIST_PLAN`] = resolve(dir, 'restart-list.plan')]);
  // NavigationTabsIOSTests': the selected tab tapped again over a list its press asks a row of.
  contract([resolve(root, 'host/apple/tests/fixtures/tab-top-request.contract'), '-o', env[`${prefix}_TAB_TOP_REQUEST_PLAN`] = resolve(dir, 'tab-top-request.plan')]);
  // TabStacksIOSTests': a tabpanel inside its root's tablist.
  contract([resolve(root, 'host/apple/tests/fixtures/nested-panel-tabs.contract'), '-o', env[`${prefix}_NESTED_PANEL_TABS_PLAN`] = resolve(dir, 'nested-panel-tabs.plan')]);
  // NavigationMoreIOSTests': more tabs than an iPhone's tab bar shows, so UIKit lists some under More.
  contract([resolve(root, 'host/apple/tests/fixtures/more-tabs.contract'), '-o', env[`${prefix}_MORE_TABS_PLAN`] = resolve(dir, 'more-tabs.plan')]);
  // NotificationActionTests': a post's declared actions and the root's handler.
  contract([resolve(root, 'host/apple/tests/fixtures/notification-actions.contract'), '-o', env[`${prefix}_NOTIFICATION_ACTIONS_PLAN`] = resolve(dir, 'notification-actions.plan')]);
  if (prefix === 'EXACT') contract([resolve(root, 'host/apple/tests/fixtures/refused-picture.contract'), '-o', env.EXACT_REFUSED_PICTURE_PLAN = resolve(dir, 'refused-picture.plan')]);
  // Written in this checkout, not beside the plan: its `use … from "@exact/notifications"` resolves from a
  // node_modules above it, which a CARGO_TARGET_DIR outside the checkout lacks. Renamed into place whole: runs with
  // other target directories share the name, and a compile must never read it half written.
  const noActions = resolve(root, 'target/fixture-sources', `${prefix}-notification-no-actions.contract`), writing = `${noActions}.${process.pid}`;
  mkdirSync(dirname(noActions), { recursive: true });
  writeFileSync(writing, readFileSync(resolve(root, 'host/apple/tests/fixtures/notification-actions.contract'), 'utf8').replace(/, actions=\[.*\]/, ''));
  renameSync(writing, noActions);
  contract([noActions, '-o', env[`${prefix}_NOTIFICATION_NO_ACTIONS_PLAN`] = resolve(dir, 'notification-no-actions.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/feedback-race.contract'), '-o', env[`${prefix}_FEEDBACK_RACE_PLAN`] = resolve(dir, 'feedback-race.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/async-advance.contract'), '-o', env[`${prefix}_ASYNC_ADVANCE_PLAN`] = resolve(dir, 'async-advance.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/save-text.contract'), '-o', env[`${prefix}_SAVE_TEXT_PLAN`] = resolve(dir, 'save-text.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/timer-admission.contract'), '-o', env[`${prefix}_TIMER_ADMISSION_PLAN`] = resolve(dir, 'timer-admission.plan')]);
  // NativeTargetIOSTests': four boxes, each padding one edge by a registered target's inset.
  contract([resolve(root, 'host/apple/tests/fixtures/registered-target.contract'), '-o', env[`${prefix}_REGISTERED_TARGET_PLAN`] = resolve(dir, 'registered-target.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/reset-gate.contract'), '-o', env[`${prefix}_RESET_GATE_PLAN`] = resolve(dir, 'reset-gate.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/still-page.contract'), '-o', env[`${prefix}_STILL_PAGE_PLAN`] = resolve(dir, 'still-page.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/reset-gate-admission.contract'), '-o', env[`${prefix}_RESET_GATE_ADMISSION_PLAN`] = resolve(dir, 'reset-gate-admission.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/native-retry.contract'), '-o', env[`${prefix}_NATIVE_RETRY_PLAN`] = resolve(dir, 'native-retry.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/dimmed-pixels.contract'), '-o', env[`${prefix}_DIMMED_PIXELS_PLAN`] = resolve(dir, 'dimmed-pixels.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/glass-title.contract'), '-o', env[`${prefix}_GLASS_TITLE_PLAN`] = resolve(dir, 'glass-title.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/reset-hold.contract'), '-o', env[`${prefix}_RESET_HOLD_PLAN`] = resolve(dir, 'reset-hold.plan')]);
  // UntitledBarIOSTests': a header of a button alone, the bar with no title.
  contract([resolve(root, 'host/apple/tests/fixtures/untitled-bar.contract'), '-o', env[`${prefix}_UNTITLED_BAR_PLAN`] = resolve(dir, 'untitled-bar.plan')]);
  // NavigationBarIOSTests': a header whose controls share and add a control.
  contract([resolve(root, 'host/apple/tests/fixtures/bar-share.contract'), '-o', env[`${prefix}_BAR_SHARE_PLAN`] = resolve(dir, 'bar-share.plan')]);
  // AgentBarPointerIOSTests': a header of more buttons than an iPad's bar places.
  contract([resolve(root, 'host/apple/tests/fixtures/bar-overflow.contract'), '-o', env[`${prefix}_BAR_OVERFLOW_PLAN`] = resolve(dir, 'bar-overflow.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/bar-hosted.contract'), '-o', env[`${prefix}_BAR_HOSTED_PLAN`] = resolve(dir, 'bar-hosted.plan')]);
  // NavigationBarIOSTests': a header's title drawn as a face, in an ink per scheme.
  contract([resolve(root, 'host/apple/tests/fixtures/bar-face-scheme.contract'), '-o', env[`${prefix}_BAR_FACE_SCHEME_PLAN`] = resolve(dir, 'bar-face-scheme.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/dialog-sheet-reads.contract'), '-o', env[`${prefix}_DIALOG_SHEET_READS_PLAN`] = resolve(dir, 'dialog-sheet-reads.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/route-backdrop.contract'), '-o', env[`${prefix}_ROUTE_BACKDROP_PLAN`] = resolve(dir, 'route-backdrop.plan')]);
  // PopoverRouteIOSTests': a route presented as a popover anchored to a button (LLP 1123 D7).
  contract([resolve(root, 'host/apple/tests/fixtures/popover-route.contract'), '-o', env[`${prefix}_POPOVER_ROUTE_PLAN`] = resolve(dir, 'popover-route.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/trailing-time.contract'), '-o', env[`${prefix}_TRAILING_TIME_PLAN`] = resolve(dir, 'trailing-time.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/intersect-route.contract'), '-o', env[`${prefix}_INTERSECT_ROUTE_PLAN`] = resolve(dir, 'intersect-route.plan')]);
  // KeyboardSheetIOSTests': a sheet's field, its keyboard toolbar and the keyboard's overlap as a value.
  contract([resolve(root, 'host/apple/tests/fixtures/keyboard-sheet.contract'), '-o', env[`${prefix}_KEYBOARD_SHEET_PLAN`] = resolve(dir, 'keyboard-sheet.plan')]);
  // PlatformDefaultsIOSTests': Next fields whose submit moves the focus through the runner.
  contract([resolve(root, 'host/apple/tests/fixtures/return-focus.contract'), '-o', env[`${prefix}_RETURN_FOCUS_PLAN`] = resolve(dir, 'return-focus.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/view-transition.contract'), '-o', env[`${prefix}_VIEW_TRANSITION_PLAN`] = resolve(dir, 'view-transition.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/route-overflow.contract'), '-o', env[`${prefix}_ROUTE_OVERFLOW_PLAN`] = resolve(dir, 'route-overflow.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/progress-contexts.contract'), '-o', env[`${prefix}_PROGRESS_CONTEXTS_PLAN`] = resolve(dir, 'progress-contexts.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/agent-unmounted.contract'), '-o', env[`${prefix}_AGENT_UNMOUNTED_PLAN`] = resolve(dir, 'agent-unmounted.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/fractional-borders.contract'), '-o', env[`${prefix}_FRACTIONAL_BORDERS_PLAN`] = resolve(dir, 'fractional-borders.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/painted-groups.contract'), '-o', env[`${prefix}_PAINTED_GROUPS_PLAN`] = resolve(dir, 'painted-groups.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/top-layer-pins.contract'), '-o', env[`${prefix}_TOP_LAYER_PINS_PLAN`] = resolve(dir, 'top-layer-pins.plan')]);
  // PositionAreaRegionIOSTests' pages: Chrome's position-area table, and mirrored.
  contract([resolve(root, 'host/apple/tests/fixtures/position-area-region.contract'), '-o', env[`${prefix}_POSITION_AREA_PLAN`] = resolve(dir, 'position-area-region.plan')]);
  contract([resolve(root, 'host/apple/tests/fixtures/position-area-region-rtl.contract'), '-o', env[`${prefix}_POSITION_AREA_RTL_PLAN`] = resolve(dir, 'position-area-region-rtl.plan')]);
  const question = 'host/apple/tests/ExactKitTests/SnapshotFixtures/open-question';
  const refusing = resolve(dir, 'entries/refusing');
  mkdirSync(refusing, { recursive: true });
  writeFileSync(resolve(refusing, 'app.contract'), 'shape Props\n  title: string\nshape Rows\n  rows: enum<"one", "two">\ncomponent App\n  view\n    text "App"\ncomponent Card\n  props\n    title: string\n  inject\n    snapshot_configuration: Rows\n  view\n    text title\n');
  writeFileSync(resolve(refusing, 'app.json'), JSON.stringify({ name: 'Refusing refresh', app: { id: appId, name: 'Refusing refresh' }, widgets: [{ kind: 'refusing', families: ['systemSmall'], component: 'Card', props: 'Props', refresh: 'nextTrains', preview: { title: 'Preview' },
    configuration: { title: 'Card', parameters: [{ name: 'rows', type: 'enum', title: 'Rows', values: [{ value: 'one', title: '1' }, { value: 'two', title: '2' }] }] } }] }));
  // Caltrain's nextTrains takes this configuration and answers a different Props shape: the refresh must refuse.
  contract([resolve(refusing, 'app.contract'), '--entry', 'refusing', '-o', env[`${prefix}_SNAPSHOT_REFUSING_PLAN`] = resolve(dir, 'refusing.plan')]);
  // WidgetGalleryTests: an accented bar at a half through the agent's widget screenshot.
  const barCard = resolve(dir, 'entries/bar-card');
  mkdirSync(barCard, { recursive: true });
  writeFileSync(resolve(barCard, 'app.contract'), 'shape CardProps\n  title: string\ncomponent App\n  view\n    text "App"\ncomponent BarCard\n  props\n    title: string\n  view\n    column background-color="#ffffff" padding=8\n      progress value=0.5 accent-color="#ff0000" width=200\n');
  writeFileSync(resolve(barCard, 'app.json'), JSON.stringify({ name: 'Bar card', app: { id: appId, name: 'Bar card' }, widgets: [{ kind: 'bar-card', families: ['systemMedium'], component: 'BarCard', props: 'CardProps', refresh: 'nextTrains', preview: { title: 'Preview' } }] }));
  contract([resolve(barCard, 'app.contract'), '--entry', 'bar-card', '-o', env[`${prefix}_SNAPSHOT_BAR_CARD_PLAN`] = resolve(dir, 'bar-card.plan')]);
  // An activity whose Lock Screen placement has a background and a link (ActivityDriverTests: the tint, the URL).
  // A configured widget (WidgetGalleryTests): its render reads the chosen rows, its defaults unless chosen, and the
  // chosen book's id.
  const configured = resolve(dir, 'entries/configured');
  mkdirSync(configured, { recursive: true });
  writeFileSync(resolve(configured, 'app.contract'), 'shape Props\n  title: string\nshape Rows\n  rows: enum<"automatic", "one", "three">\n  book: option<string>\ncomponent App\n  view\n    text "App"\ncomponent Card\n  props\n    title: string\n  inject\n    snapshot_configuration: Rows\n  view\n    column\n      text `${title} ${snapshot_configuration.rows}`\n      text (match snapshot_configuration.book { case some(id) => `Book ${id}`, case none => "No book" })\n');
  writeFileSync(resolve(configured, 'app.json'), JSON.stringify({ name: 'Configured widget', app: { id: appId, name: 'Configured widget' }, intents: { entities: { book: { title: 'Book' } } }, widgets: [{ kind: 'configured', families: ['systemSmall', 'systemMedium'], component: 'Card', props: 'Props', refresh: 'nextTrains', preview: { title: 'Rows' },
    configuration: { title: 'Card', parameters: [{ name: 'rows', type: 'enum', title: 'Rows', values: [{ value: 'automatic', title: 'Automatic' }, { value: 'one', title: '1' }, { value: 'three', title: '3' }] },
      { name: 'book', type: 'entity', title: 'Book', entity: 'book' }] } }] }));
  contract([resolve(configured, 'app.contract'), '--entry', 'configured', '-o', env[`${prefix}_SNAPSHOT_CONFIGURED_PLAN`] = resolve(dir, 'configured.plan')]);
  const tinted = resolve(dir, 'entries/tinted');
  mkdirSync(tinted, { recursive: true });
  writeFileSync(resolve(tinted, 'app.contract'), 'shape TintedState\n  title: string\ncomponent App\n  view\n    text "App"\ncomponent Tinted\n  props\n    title: string\n  view\n    view slot="lockScreen" background-color="#1e3a5f" padding=8\n      link href="/alerts" display="block"\n        text title color="#ffffff"\n    view slot="compactLeading"\n      text "T"\n    view slot="compactTrailing"\n      text title text-align="center"\n    view slot="minimal"\n      text "T"\n    view slot="expanded"\n      view slot="leading"\n        text title\n      view slot="trailing"\n        text title width=120\n      view slot="center"\n        link href="/alerts" display="block"\n          text title\n      view slot="bottom"\n        text title\n');
  writeFileSync(resolve(tinted, 'app.json'), JSON.stringify({ name: 'Tinted activity', app: { id: appId, name: 'Tinted activity', origin: 'https://tinted.exact.invalid' }, live_activities: [{ kind: 'tinted', component: 'Tinted', state: 'TintedState', preview: { title: 'Preview title' } }] }));
  contract([resolve(tinted, 'app.contract'), '--entry', 'tinted', '-o', env[`${prefix}_SNAPSHOT_TINTED_PLAN`] = resolve(dir, 'tinted.plan')]);
  // An activity whose Lock Screen root has a gradient and a colour (the colour is the tint, the gradient paints).
  const graded = resolve(dir, 'entries/graded');
  mkdirSync(graded, { recursive: true });
  writeFileSync(resolve(graded, 'app.contract'), 'shape GradedState\n  title: string\ncomponent App\n  view\n    text "App"\ncomponent Graded\n  props\n    title: string\n  view\n    view slot="lockScreen" background-color="#1e3a5f" background-image="linear-gradient(to right, #ff0000, #0000ff)"\n      text title color="#ffffff"\n    view slot="compactLeading"\n      text "G"\n    view slot="compactTrailing"\n      text title\n    view slot="minimal"\n      text "G"\n    view slot="expanded"\n      view slot="leading"\n        text title\n      view slot="trailing"\n        text title\n      view slot="center"\n        text title\n      view slot="bottom"\n        text title\n');
  writeFileSync(resolve(graded, 'app.json'), JSON.stringify({ name: 'Graded activity', app: { id: appId, name: 'Graded activity' }, live_activities: [{ kind: 'graded', component: 'Graded', state: 'GradedState', preview: { title: 'Preview title' } }] }));
  contract([resolve(graded, 'app.contract'), '--entry', 'graded', '-o', env[`${prefix}_SNAPSHOT_GRADED_PLAN`] = resolve(dir, 'graded.plan')]);
  // An activity exercising the sizes a region takes (ActivityDriverTests): root margins, `flex-basis: 0` beside an
  // authored width, a `min-width` past the proposal, a percentage of the proposal, `margin: auto`, justified content,
  // a wrapping paragraph, viewport-dependent content; and no `data-privacy` word, so the Lock Screen redacts it.
  const sized = resolve(dir, 'entries/sized');
  mkdirSync(sized, { recursive: true });
  writeFileSync(resolve(sized, 'app.contract'), 'shape SizedState\n  title: string\n  long: string\nshape SizedViewport\n  width: number\n  height: number\ncomponent App\n  view\n    text "App"\ncomponent Sized\n  props\n    title: string\n    long: string\n  inject\n    snapshot_viewport: SizedViewport\n  view\n    view slot="lockScreen" margin=12\n      text title\n      text long\n    view slot="compactLeading" width=120 min-width=0 flex-basis=0\n      text title\n    view slot="compactTrailing" min-width=200\n      text title\n    view slot="minimal" width="50%"\n      text title\n    view slot="expanded"\n      view slot="leading" display="flex" flex-direction="row" justify-content="flex-end"\n        text title\n      view slot="trailing"\n        text long\n      view slot="center" margin="auto"\n        text title\n      view slot="bottom"\n        text title\n        if snapshot_viewport.width > 160\n          text long\n');
  writeFileSync(resolve(sized, 'app.json'), JSON.stringify({ name: 'Sized activity', app: { id: appId, name: 'Sized activity' }, live_activities: [{ kind: 'sized', component: 'Sized', state: 'SizedState', preview: { title: 'Hi', long: 'A long paragraph that wraps to several lines in any Island region or Lock Screen card.' } }] }));
  contract([resolve(sized, 'app.contract'), '--entry', 'sized', '-o', env[`${prefix}_SNAPSHOT_SIZED_PLAN`] = resolve(dir, 'sized.plan')]);
  const edges = resolve(dir, 'entries/activity-edges');
  mkdirSync(edges, { recursive: true });
  cpSync(resolve(root, 'host/apple/tests/fixtures/activity-edges.contract'), resolve(edges, 'app.contract'));
  writeFileSync(resolve(edges, 'app.json'), JSON.stringify({ app: { id: appId, name: 'Activity edges' }, live_activities: [{ kind: 'edges', component: 'Edges', state: 'EdgeState', preview: { title: 'Private account label' } }] }));
  contract([resolve(edges, 'app.contract'), '--entry', 'edges', '-o', env[`${prefix}_SNAPSHOT_EDGES_PLAN`] = resolve(dir, 'edges.plan')]);
  // WidgetGalleryTests: accented bars through the agent's activity screenshot.
  const bars = resolve(dir, 'entries/activity-bars');
  mkdirSync(bars, { recursive: true });
  cpSync(resolve(root, 'host/apple/tests/fixtures/activity-bars.contract'), resolve(bars, 'app.contract'));
  writeFileSync(resolve(bars, 'app.json'), JSON.stringify({ app: { id: appId, name: 'Activity bars' }, live_activities: [{ kind: 'bars', component: 'Bars', state: 'BarsState', preview: { start: 0, end: 1 } }] }));
  contract([resolve(bars, 'app.contract'), '--entry', 'bars', '-o', env[`${prefix}_SNAPSHOT_BARS_PLAN`] = resolve(dir, 'bars.plan')]);
  if (prefix === 'EXACT') {
    const app = resolve(dir, 'SnapshotAssets.app'), resources = resolve(app, 'Contents/Resources');
    const extension = resolve(app, 'Contents/PlugIns/Card.appex');
    const catalog = resolve(dir, 'SnapshotAssets.xcassets');
    for (const path of [resources, resolve(resources, 'assets'), resolve(extension, 'Contents/Resources'), resolve(catalog, 'WidgetInk.colorset')]) mkdirSync(path, { recursive: true });
    for (const [bundle, id] of [[app, 'dev.exact.snapshot-assets'], [extension, 'dev.exact.snapshot-assets.widget']])
      writeFileSync(resolve(bundle, 'Contents/Info.plist'), plistFile({ CFBundleIdentifier: id, CFBundlePackageType: bundle === app ? 'APPL' : 'XPC!', CFBundleExecutable: 'Fixture' }));
    writeFileSync(resolve(catalog, 'Contents.json'), JSON.stringify({ info: { version: 1, author: 'xcode' } }));
    writeFileSync(resolve(catalog, 'WidgetInk.colorset/Contents.json'), JSON.stringify({ info: { version: 1, author: 'xcode' }, colors: [{ idiom: 'universal', color: { 'color-space': 'srgb', components: { red: '1.0', green: '0.0', blue: '0.0', alpha: '1.0' } } }] }));
    const rasterApp = { dir: iconApp, manifest: {}, appleIcons: [{ svg: 'two.svg', viewBox: '0 0 24 24' }] };
    rasterIconAssets(appleAssetCatalogInventory(rasterApp, 'macos'), rasterApp, catalog);
    run('xcrun', ['actool', '--compile', resources, '--platform', 'macosx', '--minimum-deployment-target', '14.0', '--target-device', 'mac', catalog]);
    writeFileSync(resolve(resources, 'assets/widget.png'), encodePng({ width: 1, height: 1, data: Buffer.from([255, 0, 0, 255]) }));
    env.EXACT_SNAPSHOT_ASSETS_EXTENSION = extension;
  }
  const copies = new Map();
  for (const [app, kind, key] of [['apps/caltrain', 'next-trains', 'SNAPSHOT_TRAINS'], ['apps/caltrain', 'departure', 'SNAPSHOT_DEPARTURE'], ['apps/caltrain', 'service-alert', 'SNAPSHOT_SERVICE_ALERT'], [question, 'question-small', 'SNAPSHOT_QUESTION_SMALL'],
    [question, 'question-medium', 'SNAPSHOT_QUESTION_MEDIUM'], [question, 'question-rectangular', 'SNAPSHOT_QUESTION_RECTANGULAR'], [question, 'question-adaptive', 'SNAPSHOT_QUESTION_ADAPTIVE'], [question, 'gradient-card', 'SNAPSHOT_GRADIENT']]) {
    let copy = copies.get(app);
    if (!copy) {
      copy = resolve(dir, 'entries', app.split('/').pop());
      mkdirSync(copy, { recursive: true });
      for (const file of readdirSync(resolve(root, app)).filter(f => f.endsWith('.contract'))) cpSync(resolve(root, app, file), resolve(copy, file));
      const manifest = JSON.parse(readFileSync(resolve(root, app, 'app.json'), 'utf8'));
      writeFileSync(resolve(copy, 'app.json'), JSON.stringify({ ...manifest, $schema: undefined, id: appId, app: { ...manifest.app, id: appId } }));
      copies.set(app, copy);
    }
    contract([resolve(copy, 'app.contract'), '--entry', kind, '-o', env[`${prefix}_${key}_PLAN`] = resolve(dir, `${kind}.plan`)]);
  }
}

/** The Swift host tests on this Mac, built under `scratch`: each test class
 *  in an `xctest` process of its own, as many at once as there are cores, so
 *  a crash ends its class and not the run. The classes whose files use what
 *  every process on the Mac shares (the general pasteboard, the active app,
 *  the mouse, a capture of the screen) share one process beside them, one
 *  class after another. A window's key status and first responder belong
 *  to its own process. A class whose file opens a menu that tracks, which
 *  it observes as `NSMenu.didBeginTrackingNotification` with
 *  `addObserver(forName:)`, runs after all of them, alone, in a process of its
 *  own: beside another test process its menu can close before the test reads
 *  it, and in the shared process it reads the menu items other classes
 *  left. The Canvas 2D GPU module (`canvasGpu`) is loaded only in its own
 *  tests' process: loaded, every canvas draws on the GPU. `only`, when
 *  given, names the classes to run. */
export async function macTests(pkg, scratch, env, canvasGpu, only = null) {
  const listed = read('swift', ['test', '--skip-build', '--list-tests', '--scratch-path', scratch], { cwd: pkg, env });
  if (listed.status !== 0) throw new Error(`swift test --list-tests failed: ${listed.stderr}`);
  const classes = chosen([...new Set(listed.stdout.match(/^ExactKitTests\.\w+(?=\/)/gm) ?? [])], only, 'macOS');
  const macWide = /NSPasteboard\.general|\bactivate\(|NSEvent\.mouseLocation|CGWindowListCreateImage/;
  const tracksMenu = /addObserver\(forName: NSMenu\.didBeginTrackingNotification/;
  const shared = new Set(), solo = new Set();
  for (const file of readdirSync(resolve(pkg, 'tests/ExactKitTests')).filter(f => f.endsWith('.swift'))) {
    const source = readFileSync(resolve(pkg, 'tests/ExactKitTests', file), 'utf8');
    const named = [...source.matchAll(/class (\w+)\s*:\s*XCTestCase/g)].map(([, name]) => `ExactKitTests.${name}`);
    for (const name of named) if (tracksMenu.test(source)) solo.add(name); else if (macWide.test(source)) shared.add(name);
  }
  const gpuClass = 'ExactKitTests.Canvas2DGpuStressTests';
  shared.delete(gpuClass);
  const groups = [classes.filter(c => shared.has(c)), ...classes.filter(c => !shared.has(c) && !solo.has(c)).map(c => [c])].filter(g => g.length);
  const alone = classes.filter(c => solo.has(c)).map(c => [c]);
  // SwiftPM names the test bundle after the test target up to Swift 6.2 and after the package from 6.3 on.
  const bin = read('swift', ['build', '--show-bin-path', '--scratch-path', scratch], { cwd: pkg, env }).stdout.trim();
  const bundle = ['ExactKitTests.xctest', 'ExactPackageTests.xctest'].map(name => resolve(bin, name)).find(existsSync) ?? resolve(bin, 'ExactKitTests.xctest');
  const failed = [];
  const one = (group) => new Promise((done) => {
    const gpu = group.includes(gpuClass) ? { EXACT_CANVAS_GPU_DYLIB: canvasGpu } : {};
    const liveStore = group.some(c => c === 'ExactKitTests.LiveStoreObservationTests' || c === 'ExactKitTests.LiveScratchStoreObservationTests') ? { EXACT_AGENT: 'live', EXACT_STORE: 'real', EXACT_AGENT_STORAGE: group.includes('ExactKitTests.LiveScratchStoreObservationTests') ? 'agent2-live' : '' } : {};
    const behindWeb = group.includes('ExactKitTests.AgentWebWindowMacTests') ? { EXACT_AGENT: '1', EXACT_AGENT_TIMING: 'agent' } : {};
    const testEnv = { ...env, ...gpu, ...liveStore, ...behindWeb };
    if (group.includes('ExactKitTests.LiveStoreObservationTests')) delete testEnv.EXACT_AGENT_STORAGE;
    const child = spawn('xcrun', ['xctest', '-XCTest', group.join(','), bundle], { cwd: pkg, env: testEnv, stdio: ['ignore', 'pipe', 'pipe'] });
    let out = '';
    child.stdout.on('data', (d) => { out += d; });
    child.stderr.on('data', (d) => { out += d; });
    child.on('error', (e) => { out += `${e.message}\n`; });
    child.on('close', (status, signal) => {
      process.stdout.write(out);
      if (status !== 0) failed.push(`${group.join(', ')} (xctest ${signal ?? `exited ${status}`})`);
      done();
    });
  });
  // One queue, several takers: `next` is read and advanced with no await between.
  let next = 0;
  const taker = async () => { while (next < groups.length) await one(groups[next++]); };
  await Promise.all(Array.from({ length: Math.min(groups.length, availableParallelism()) }, taker));
  for (const group of alone) await one(group);
  console.log(`host/apple: ${classes.length} test classes in ${groups.length + alone.length} processes (${alone.length} alone after the rest), ${failed.length} failed`);
  if (failed.length) throw new Error(`swift tests failed:\n  ${failed.join('\n  ')}`);
}

/** Builds the iOS tests into `derived` and runs them on the simulator `udid` in separate runs that
 *  always all run; reports their failures together. Most run bare in the xctest agent.
 *  Those that need what only an app gives (a window scene to present in, UIKit's accessibility
 *  runtime; `hostedTests`) run hosted in an app with one scene (`hostSource`, built here) with the
 *  accessibility runtime on for that run alone. The Canvas 2D GPU module's own run apart with it
 *  loaded (`canvasGpu`): loaded, every canvas draws on the GPU, and the others read the Core
 *  Graphics path a runner without it draws. `runApple` and `run` are build.mjs's: they throw on
 *  failure, and `runApple` refuses a mixed deployment target. `only`, when
 *  given, names the classes to run, and a run with none of them is left out. Tests needing
 *  real touches run without simulator clones, beside the existing XCTest touch runner. Every run
 *  writes its result bundle into `derived`, beside the build. After the last run, the simulator's
 *  clones left over from test runs in the device set `testingDevices` are deleted
 *  (`deleteLeftoverClones`). */
export async function iosTestRuns({ pkg, derived, destination, udid, env, canvasGpu, hostSource, triple, runApple, run, only = null, testingDevices = TESTING_DEVICES }) {
  // Build without code coverage, which the package's generated scheme turns on: instrumented tests
  // write tens of megabytes of profiles per run, and nothing in Exact reads them
  runApple('xcodebuild', ['build-for-testing', '-scheme', 'Exact-Package', '-destination', destination, '-derivedDataPath', derived, '-enableCodeCoverage', 'NO'], { cwd: pkg, stdio: 'inherit', env });
  const products = resolve(derived, 'Build/Products');
  const bare = withoutPerformanceChecker(products), host = resolve(products, 'ExactTestHost.app');
  rmSync(host, { recursive: true, force: true });
  mkdirSync(host, { recursive: true });
  writeFileSync(resolve(host, 'Info.plist.json'), JSON.stringify({ CFBundleIdentifier: 'com.exact.testhost', CFBundleExecutable: 'ExactTestHost', CFBundleName: 'ExactTestHost',
    CFBundlePackageType: 'APPL', CFBundleVersion: '1', CFBundleShortVersionString: '1.0', CFBundleSupportedPlatforms: ['iPhoneSimulator'], MinimumOSVersion: '17.0',
    LSRequiresIPhoneOS: true, UIDeviceFamily: [1, 2], UILaunchScreen: {}, UIApplicationSceneManifest: { UIApplicationSupportsMultipleScenes: false } }));
  run('plutil', ['-convert', 'xml1', '-o', resolve(host, 'Info.plist'), resolve(host, 'Info.plist.json')]);
  rmSync(resolve(host, 'Info.plist.json'));
  runApple('xcrun', ['--sdk', 'iphonesimulator', 'swiftc', '-parse-as-library', '-target', triple, hostSource, '-o', resolve(host, 'ExactTestHost')]);
  const hosted = hostedIn(products, bare, 'ExactTestHost.app');
  run('codesign', ['-s', '-', '--force', '--deep', host], { stdio: 'ignore' });
  const gpuTests = 'ExactKitTests/Canvas2DGpuStressTests', classes = only?.map(c => `ExactKitTests/${c}`);
  const testDir = resolve(pkg, 'tests/ExactKitTests');
  const selected = t => !classes || classes.some(c => t.startsWith(`${c}/`));
  const touchOnly = hostedTests(testDir, 'touch').filter(selected);
  const hostedOnly = hostedTests(testDir).filter(selected).filter(t => !touchOnly.includes(t));
  const failures = [];
  // A test still running at its allowance fails, and the run goes on with the next test in a new
  // process: a test that hangs (as a share sheet waiting on a simulator's `sharingd` can) costs its
  // allowance, not the rest of the run
  const testRun = (xctestrun, selection, runEnv, parallel = true) => {
    try {
      // Name the build's derived data: without `-derivedDataPath`, xcodebuild starts a folder of its
      // own in `~/Library/Developer/Xcode/DerivedData` for every call
      runApple('xcodebuild', ['test-without-building', '-xctestrun', xctestrun, '-derivedDataPath', derived, '-destination', destination, '-collect-test-diagnostics', 'never',
        '-test-timeouts-enabled', 'YES', '-default-test-execution-time-allowance', '600',
        '-parallel-testing-enabled', parallel ? 'YES' : 'NO', ...(parallel ? ['-parallel-testing-worker-count', '2'] : []), ...selection], { cwd: pkg, stdio: 'inherit', env: runEnv });
    } catch (e) { failures.push(e); }
  };
  const bareClasses = classes?.filter(c => c !== gpuTests);
  if (bareClasses?.length !== 0) testRun(bare, [...(bareClasses ?? []).map(c => `-only-testing:${c}`),
    ...[gpuTests, ...hostedOnly, ...touchOnly].map(t => `-skip-testing:${t}`)], env);
  // UIKit loads its accessibility code only for an assistive technology or automation
  // (AgentAccessibility.axRuntimeLoaded): on for the hosted run, then as it was.
  if (hostedOnly.length) {
    const ax = ['com.apple.Accessibility', 'ApplicationAccessibilityEnabled'];
    const { markHostedAccessibility, withAccessibilityLock } = await import('./touches.mjs');
    let before, unmark;
    try {
      run('xcrun', ['simctl', 'bootstatus', udid, '-b']);
      await withAccessibilityLock(udid, () => {
        before = read('xcrun', ['simctl', 'spawn', udid, 'defaults', 'read', ...ax]);
        if (before.status === 0 && before.stdout.trim() === '1') return;
        unmark = markHostedAccessibility(udid, before.status === 0 ? '0' : undefined);
        run('xcrun', ['simctl', 'spawn', udid, 'defaults', 'write', ...ax, '-bool', 'true']);
      });
      testRun(hosted, hostedOnly.map(t => `-only-testing:${t}`), env);
    } catch (error) { failures.push(error); }
    finally {
      if (unmark) {
        const restore = () => {
          const release = unmark;
          unmark = null;
          try { read('xcrun', ['simctl', 'spawn', udid, 'defaults', ...(before.status === 0 ? ['write', ...ax, '-bool', 'false'] : ['delete', ...ax])]); }
          catch (error) { failures.push(error); }
          finally { release(); }
        };
        try { await withAccessibilityLock(udid, restore); }
        catch (error) {
          if (unmark) restore(); // A busy display must not strand the setting or the hosted marker.
          else failures.push(error);
        }
      }
    }
  }
  if (touchOnly.length) {
    let touches;
    try {
      run('xcrun', ['simctl', 'bootstatus', udid, '-b']);
      const { openTouches } = await import('./touches.mjs');
      touches = await openTouches({ udid, appId: 'com.exact.testhost' });
      const { port, token } = await touches.handoff();
      testRun(hosted, touchOnly.map(t => `-only-testing:${t}`), {
        ...env, TEST_RUNNER_EXACT_TOUCH_PORT: String(port), TEST_RUNNER_EXACT_TOUCH_TOKEN: token,
      }, false);
    } catch (error) { failures.push(error); }
    finally {
      try {
        if (touches) {
          if (touches.kept) await touches.close();
          else {
            const failed = await touches.end();
            if (failed) throw failed;
          }
        }
      } catch (error) { failures.push(error); }
    }
  }
  if (!classes || classes.includes(gpuTests)) testRun(bare, [`-only-testing:${gpuTests}`], { ...env, TEST_RUNNER_EXACT_CANVAS_GPU_DYLIB: canvasGpu });
  // Warn of clones left over rather than fail: they fail no test
  try {
    deleteLeftoverClones(testingDevices, udid);
  } catch (e) {
    console.warn(`host/apple: leftover clones of the iOS tests' simulator may remain in ${testingDevices}: ${e.message}. List them with \`xcrun simctl --set ${testingDevices} list devices\` and delete one with \`xcrun simctl --set ${testingDevices} delete <udid>\`, or the next iOS test run on this simulator tries again`);
  }
  if (failures.length === 1) throw failures[0];
  if (failures.length) throw new AggregateError(failures, `iOS tests failed:\n  ${failures.map(e => e.message).join('\n  ')}`);
}

/** Deletes the clones of the simulator `udid` left over from test runs in the device set at `set`.
 *  xcodebuild clones its destination for each parallel test runner while the tests run, and
 *  CoreSimulator deletes those clones once xcodebuild exits, but not one still being made then:
 *  CoreSimulator finishes that one after xcodebuild has gone, and it stays (Xcode 27.1). A clone
 *  has its simulator's runtime and device type and is named `Clone <n> of <the simulator's name>`.
 *  Another run may be testing on this simulator: xcodebuild shuts its destination down while it
 *  tests on the clones, and `build.mjs` chooses a simulator that is not booted. That run's clones
 *  can be shut down too: a clone is shut down before its runner starts and after it finishes, and
 *  goes only when xcodebuild exits. So nothing is deleted while an xcodebuild that may clone
 *  (`mayClone`) names the simulator, by its UDID or its name, or another simulator with its name,
 *  runtime and type, whose clones look the same. Otherwise every shut-down clone of the simulator
 *  is left over. A parallel test run in Xcode itself is not seen, and it can lose a shut-down
 *  clone if it overlaps an iOS test run of `build.mjs` on the same simulator. A clone still being
 *  made as the last run ends is not listed yet, so a later run on the simulator deletes it, and the
 *  last clone of a simulator that is then deleted or renamed stays. */
function deleteLeftoverClones(set, udid) {
  if (!existsSync(set)) return;
  const defaults = simulators(), simulator = defaults.find(d => d.udid === udid);
  if (!simulator) return;
  const alike = d => d.runtime === simulator.runtime && d.type === simulator.type;
  const named = [simulator.name, ...defaults.filter(d => d.name === simulator.name && alike(d)).map(d => d.udid)];
  const processes = read('ps', ['-axww', '-o', 'command=']);
  if (processes.status !== 0) throw new Error(`\`ps\` could not list the running xcodebuilds: ${processes.stderr || processes.error?.message}`);
  if (processes.stdout.split('\n').some(command => mayClone(command) && named.some(word => command.includes(word)))) return;
  const failed = [];
  for (const clone of simulators(set).filter(d => d.state === 'Shutdown' && alike(d) && /^Clone \d+ of (.*)$/.exec(d.name)?.[1] === simulator.name)) {
    const deleted = read('xcrun', ['simctl', '--set', set, 'delete', clone.udid]);
    // Skip a clone another run's cleanup deleted meanwhile
    if (deleted.status !== 0 && existsSync(resolve(set, clone.udid))) failed.push(`${clone.udid} (${(deleted.stderr || deleted.error?.message || '').trim()})`);
  }
  if (failed.length) throw new Error(`\`xcrun simctl delete\` failed for ${failed.join(', ')}`);
}

/** Whether the command line `command` is an xcodebuild that may clone its destination: a test
 *  action without parallel testing turned off. */
function mayClone(command) {
  const words = command.split(/\s+/), parallel = words.lastIndexOf('-parallel-testing-enabled');
  return words.some(word => word === 'xcodebuild' || word.endsWith('/xcodebuild')) && (words.includes('test') || words.includes('test-without-building'))
    && !(parallel >= 0 && words[parallel + 1] === 'NO');
}

/** The newest `.xctestrun` `build-for-testing` wrote in `products`, as
 *  `exact-tests.xctestrun` beside it without the Thread Performance Checker:
 *  the scheme Xcode makes for the package inserts it (libRPAC), and it
 *  writes a symbolicated backtrace at every wait on a thread of lower
 *  priority, which is most of the button tests' time, and fails no test.
 *  The Main Thread Checker stays. Returns the new file's path. */
export function withoutPerformanceChecker(products) {
  const built = readdirSync(products).filter(f => f.endsWith('.xctestrun') && f !== 'exact-tests.xctestrun')
    .map(f => resolve(products, f)).sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)[0];
  if (!built) throw new Error(`no .xctestrun in ${products}`);
  const converted = read('plutil', ['-convert', 'json', '-o', '-', built]);
  if (converted.status !== 0) throw new Error(`plutil ${built}: ${converted.stderr}`);
  const plan = JSON.parse(converted.stdout);
  const strip = (o) => {
    if (!o || typeof o !== 'object') return;
    for (const [key, value] of Object.entries(o)) {
      if (!/^(Testing)?EnvironmentVariables$/.test(key)) { strip(value); continue; }
      for (const name of Object.keys(value)) if (name.startsWith('PERFC_')) delete value[name];
      const inserted = (value.DYLD_INSERT_LIBRARIES ?? '').split(':').filter(l => l && !l.endsWith('/libRPAC.dylib'));
      if (inserted.length) value.DYLD_INSERT_LIBRARIES = inserted.join(':'); else delete value.DYLD_INSERT_LIBRARIES;
    }
  };
  strip(plan);
  const xctestrun = resolve(products, 'exact-tests.xctestrun'), json = `${xctestrun}.json`;
  writeFileSync(json, JSON.stringify(plan));
  const written = read('plutil', ['-convert', 'xml1', '-o', xctestrun, json]);
  if (written.status !== 0) throw new Error(`plutil ${json}: ${written.stderr}`);
  return xctestrun;
}

/** `xctestrun` with its tests hosted in `app` (an iOS app bundle beside it in
 *  `products`), as Xcode runs app-hosted unit tests: the bundle is copied into
 *  the app's PlugIns and injected into it at launch. Returns the new file's path. */
export function hostedIn(products, xctestrun, app) {
  const converted = read('plutil', ['-convert', 'json', '-o', '-', xctestrun]);
  if (converted.status !== 0) throw new Error(`plutil ${xctestrun}: ${converted.stderr}`);
  const plan = JSON.parse(converted.stdout);
  for (const configuration of plan.TestConfigurations ?? []) for (const target of configuration.TestTargets ?? []) {
    const bundle = target.TestBundlePath.replace('__TESTROOT__', products), name = bundle.split('/').pop();
    mkdirSync(resolve(products, app, 'PlugIns'), { recursive: true });
    rmSync(resolve(products, app, 'PlugIns', name), { recursive: true, force: true });
    cpSync(bundle, resolve(products, app, 'PlugIns', name), { recursive: true });
    Object.assign(target, { TestHostPath: `__TESTROOT__/${app}`, TestBundlePath: `__TESTHOST__/PlugIns/${name}`, IsAppHostedTestBundle: true, DependentProductPaths: [`__TESTROOT__/${app}`] });
    const env = target.TestingEnvironmentVariables ??= {};
    env.DYLD_INSERT_LIBRARIES = ['__PLATFORMS__/iPhoneSimulator.platform/Developer/usr/lib/libXCTestBundleInject.dylib', env.DYLD_INSERT_LIBRARIES].filter(Boolean).join(':');
    env.XCInjectBundleInto = 'unused';
  }
  const hosted = resolve(products, 'exact-tests-hosted.xctestrun'), json = `${hosted}.json`;
  writeFileSync(json, JSON.stringify(plan));
  const written = read('plutil', ['-convert', 'xml1', '-o', hosted, json]);
  if (written.status !== 0) throw new Error(`plutil ${json}: ${written.stderr}`);
  return hosted;
}

/** The test classes `--only` names in `args`, `FrameHealthMetricsTests,FrameHealthIOSTests`, checked
 *  against those the files in `dir` declare for `platform`, so a misspelled class, or one the
 *  platform compiles out, fails before the build. */
export function onlyClasses(args, dir, platform) {
  const value = args[args.indexOf('--only') + 1] ?? '';
  const only = value.startsWith('-') ? [] : value.split(',').map(c => c.trim().replace(/^ExactKitTests\./, '')).filter(Boolean);
  if (!only.length) throw new Error('--only names no test class. Name one or more, separated by commas: --only FrameHealthMetricsTests,FrameHealthIOSTests');
  chosen(testClasses(dir, platform).map(c => `ExactKitTests.${c}`), only, platform);
  return only;
}

/** `classes`, `ExactKitTests.<Class>`, kept to those `only` names when it is given; a name that is no
 *  test class there fails, naming the platform. */
function chosen(classes, only, platform) {
  if (!only) return classes;
  const unknown = only.filter(name => !classes.includes(`ExactKitTests.${name}`));
  if (unknown.length) throw new Error(`--only names ${unknown.join(', ')}, which ${unknown.length === 1 ? 'is no' : 'are not'} ${platform} test class${unknown.length === 1 ? '' : 'es'} in tests/ExactKitTests. Name a test class that ${platform} compiles, as FrameHealthMetricsTests, which both platforms run; a class within #if os() runs only where that condition holds.`);
  return only.map(name => `ExactKitTests.${name}`);
}

/** The test classes the files in `dir` declare for `platform`, 'iOS' or 'macOS', by name: a subclass
 *  of `XCTestCase`, of a fixture or of another test class, outside any `#if` branch that rules the
 *  platform out by `os()` or `canImport(UIKit|AppKit)`. A condition that names anything else is taken
 *  to hold either way. */
export function testClasses(dir, platform) {
  return declarations(dir, platform).tests;
}

/** The test classes of `platform` that a change to `files` (names in `dir`) can reach, to run
 *  alone (`--only`): those the files declare or extend, the classes of each file that names one
 *  reached (a subclass, an extension, a use of a member), and those derived from one. Null when any
 *  class can be reached: a file is gone, or it, or a file that names a class reached, declares at
 *  its top level, not `private`, something else other files can use (a function, a type that is no
 *  test class, an extension of one), as the shared helpers do; or when the files reach no class yet
 *  compile something on the platform (a `private extension` of a test class, a fixture renamed),
 *  whose error only a build would show. */
export function classesReached(dir, platform, files) {
  const { parents, tests, perFile } = declarations(dir, platform);
  const reached = new Set();
  for (const file of files) {
    const own = perFile.get(file);
    if (!own || own.shared) return null;
    for (const name of own.classes) reached.add(name);
  }
  // Reach the classes of each file that names a class reached; every class when that file is shared
  for (let grew = true; grew;) {
    grew = false;
    for (const own of perFile.values()) {
      if (![...own.names].some(name => reached.has(name))) continue;
      if (own.shared) return null;
      const before = reached.size;
      for (const name of own.classes) reached.add(name);
      grew ||= reached.size > before;
    }
  }
  // Reach a class declared indented (nested, or within `#if`) through the class it derives from
  const reaches = (name, seen = new Set()) => reached.has(name) || (parents.has(name) && !seen.has(name) && reaches(parents.get(name), seen.add(name)));
  const out = tests.filter(name => reaches(name));
  return out.length || !files.some(file => perFile.get(file).compiles) ? out : null;
}

/** A line that declares and compiles nothing: an import, a diagnostic directive (`#warning`), attributes. */
const DECLARES_NOTHING = /^\s*(?:(?:@\w+(?:\([^)]*\))?\s+)*import\b|#(?:warning|error|sourceLocation)\b|(?:@\w+(?:\([^)]*\))?\s*)+$)/;

/** What the Swift files in `dir` declare for `platform`: each class's parent (`parents`), the
 *  test classes (`tests`, sorted), and per file the classes it declares or extends at its top level,
 *  not `private` (`classes`), the capitalized names it uses (`names`), whether it compiles anything
 *  on the platform but what `DECLARES_NOTHING` matches or a block comment holds (`compiles`), and
 *  whether it declares at its top level, not `private`, anything else other files can use
 *  (`shared`). */
function declarations(dir, platform) {
  const ios = platform === 'iOS';
  // 'yes', 'no' or 'maybe' for a condition, worked out with each condition it names that does not
  // depend on the platform taken first as true and then as false
  const holds = (condition) => {
    const as = (other) => {
      const js = condition.replace(/\/\/.*$/, '').replace(/os\((\w+)\)/g, (_, os) => String(os === (ios ? 'iOS' : 'macOS')))
        .replace(/canImport\((\w+)\)/g, (_, m) => m === 'UIKit' ? String(ios) : m === 'AppKit' ? String(!ios) : String(other))
        // The iOS tests run on a simulator, and neither platform's in Mac Catalyst
        .replace(/targetEnvironment\((\w+)\)/g, (_, e) => String(e === 'simulator' && ios))
        .replace(/\b(?!true\b|false\b)[A-Za-z_]\w*\b/g, String(other));
      // Only whole tokens of a condition reach the evaluation; one that does not parse is taken either way
      if (!/^(\s*(true|false|&&|\|\||[()!]))*\s*$/.test(js)) return other;
      try { return Boolean(Function(`return (${js})`)()); } catch { return other; }
    };
    const [one, two] = [as(true), as(false)];
    return one === two ? (one ? 'yes' : 'no') : 'maybe';
  };
  const parents = new Map(), perFile = new Map();
  for (const file of readdirSync(dir).filter(f => f.endsWith('.swift'))) {
    const branches = [], own = { classes: [], names: new Set(), compiles: false, shared: false };
    perFile.set(file, own);
    let comment = false;
    for (const line of readFileSync(resolve(dir, file), 'utf8').split('\n')) {
      // Skip a block comment, from the line that opens it to the one that closes it
      if (comment || /^\s*\/\*/.test(line)) { comment = !line.includes('*/'); continue; }
      const directive = /^\s*#(if|elseif|else|endif)\b\s*(.*)/.exec(line);
      if (directive) {
        const [, word, condition] = directive, top = branches.at(-1);
        if (word === 'if') { const now = holds(condition); branches.push({ now, taken: now === 'yes', open: now !== 'no' }); }
        else if (word === 'endif') branches.pop();
        else if (top) {
          const now = top.taken ? 'no' : word === 'else' ? (top.open ? 'maybe' : 'yes') : holds(condition);
          Object.assign(top, { now, taken: top.taken || now === 'yes', open: top.open || now !== 'no' });
        }
        continue;
      }
      if (/^\s*\/\//.test(line) || branches.some(b => b.now === 'no')) continue;
      for (const name of line.match(/\b[A-Z]\w*/g) ?? []) own.names.add(name);
      if (/\S/.test(line) && !DECLARES_NOTHING.test(line)) own.compiles = true;
      const declaration = /\bclass (\w+)\s*:\s*(\w+)/.exec(line);
      if (declaration) parents.set(declaration[1], declaration[2]);
      // Read a declaration at the file's top level (unindented), after its attributes and modifiers
      const top = /^(?:@\w+(?:\([^)]*\))?\s+)*((?:\w+\s+)*?)(extension|func|struct|enum|protocol|class|actor|let|var|typealias)\s+(\w+)/.exec(line);
      // Mark the file shared at a top-level line the pattern does not read (an operator,
      // `private(set) var`): it may declare what other files use
      if (!top) { if (/^[^\s}\])]/.test(line) && !DECLARES_NOTHING.test(line)) own.shared = true; continue; }
      if (/\b(private|fileprivate)\b/.test(top[1])) continue;
      // Count an extension of a class as that class's own, as its body is
      if (top[2] === 'class' || top[2] === 'extension') own.classes.push(top[3]);
      else own.shared = true;
    }
  }
  const isTest = (name, seen = new Set()) => {
    const parent = parents.get(name);
    if (!parent || seen.has(name)) return false;
    return parent === 'XCTestCase' || /Fixture$/.test(parent) || isTest(parent, seen.add(name));
  };
  // Mark a file shared when it declares or extends a class that is no test class or fixture: other
  // files can use that type
  for (const own of perFile.values()) if (own.classes.some(name => !isTest(name))) own.shared = true;
  // A fixture is a base the test classes share, with no tests of its own
  const tests = [...parents.keys()].filter(name => isTest(name) && !name.endsWith('Fixture')).sort();
  return { parents, tests, perFile };
}

/** The tests in `dir` that need what only an app gives them, `ExactKitTests/<Class>/<test>`:
 *  those that throw `missing("scene", …)` (a presentation completes only in a scene's
 *  window) or `missing("accessibility", …)` (UIKit's accessibility runtime), which run
 *  hosted. `missing("touch", …)` selects tests that need the XCTest touch runner too.
 *  Each must say so in the test method itself, so a helper's need is not missed. A test method in
 *  an extension of a test class, in any file, is that class's. */
export function hostedTests(dir, prerequisite = 'scene|accessibility') {
  const tests = new Set();
  const requirement = new RegExp(`missing\\("(${prerequisite})"`);
  const declaration = /class (\w+)\s*:\s*(XCTestCase|\w+Fixture)\b/;
  const files = readdirSync(dir).filter(f => f.endsWith('.swift')).map(f => [f, readFileSync(resolve(dir, f), 'utf8').split('\n')]);
  const classes = new Set(files.flatMap(([, lines]) => lines.map(line => declaration.exec(line)?.[1]).filter(Boolean)));
  for (const [file, lines] of files) {
    let className = null, func = null;
    lines.forEach((line, i) => {
      const extended = /^extension (\w+)\b/.exec(line)?.[1];
      className = declaration.exec(line)?.[1] ?? (classes.has(extended) ? extended : className);
      func = /\bfunc (\w+)\s*\(/.exec(line)?.[1] ?? func;
      if (!requirement.test(line)) return;
      if (!className || !func?.startsWith('test')) throw new Error(`${file}:${i + 1}: missing(${prerequisite}) outside a test method (${func ?? 'none'}); say it in the test so the test runs hosted`);
      tests.add(`ExactKitTests/${className}/${func}`);
    });
  }
  return [...tests].sort();
}
