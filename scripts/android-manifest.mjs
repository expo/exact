// An Android implementation's manifest fragment (LLP 1133.030 D9, D12 item 4; LLP 1133.026 D19 item 3;
// LLP 1133.012 D12 item 3): the components and queries a package declares in its `exact.json` `"manifest"`, checked
// against an allow-list and written into the APK's generated manifest. Every package gets this seam (LLP 1133.001
// D8), so the list bounds what any package runs with no session and which packages and services it can see: what it
// names, and nothing the XML could say besides, is all a fragment can add. A package that needs another action or
// type amends the list in its RFC.
//
//   "manifest": {
//     "receivers": [{"name": "<class>", "directBootAware": true, "actions": ["android.intent.action.BOOT_COMPLETED"]}],
//     "services": [{"name": "<class>", "directBootAware": true, "foregroundServiceType": "systemExempted"}],
//     "queries": [{"package": "com.google.android.apps.healthdata"}, {"action": "android.intent.action.TTS_SERVICE"},
//                 {"action": "android.intent.action.SENDTO", "scheme": "mailto"}],
//     "activities": [{"name": "<class>", "actions": ["androidx.health.ACTION_SHOW_PERMISSIONS_RATIONALE"]}],
//     "aliases": [{"name": "<class>", "targetActivity": "<an activity above>", "action": "android.intent.action.VIEW_PERMISSION_USAGE",
//                  "category": "android.intent.category.HEALTH_PERMISSIONS"}],
//     "permissions": ["android.permission.ACCESS_NETWORK_STATE"],
//     "metadata": [{"name": "com.google.android.gms.wallet.api.enabled", "value": "true"},
//                  {"name": "com.google.firebase.messaging.default_notification_icon", "resource": "@drawable/exact_notification"}]
//   }
//
// Receivers and services are `exported="false"`, but for a service another app binds: the system's protected broadcasts still reach an unexported receiver,
// and a receiver for the package's own alarm needs no action, since its intent is explicit. An activity is exported,
// since the system starts it, but only for an action on the list, and never shows: it is `Theme.NoDisplay` and opens
// the app's privacy policy, which the application's `exact.privacy_policy` meta-data carries (Health Connect's
// rationale, [Health Connect: get started](https://developer.android.com/health-and-fitness/guides/health-connect/develop/get-started)).
// A service bound by another app through a permission only that app holds (`SERVICE_BINDS`: Google Pay's callbacks,
// LLP 1133.034 D5) is exported, since the binder is Play services' process; the permission is what guards it.
// A service a library's own exported receiver starts (`LIBRARY_SERVICES`: Firebase's messaging) is unexported and
// answers that library's action, and the library's `<provider>`, which would run at every process start, is removed.
// An alias is guarded by `START_VIEW_PERMISSION_USAGE`, which only the system holds. A permission is only one whose
// protection level is `normal`, on `NORMAL_PERMISSIONS` (LLP 1133.031 D12 item 1), which the system grants at install
// and asks nothing for; a runtime permission comes only from a grant line, which derives its prompt (LLP 1133.002 D6).
// There is no `<provider>` but a library's removed one, and no `android:process`.

import { EXACT_PATHS, androidAuthCallbacks } from './links.mjs';

/** The broadcasts a package's receiver may name: the boot, the package replaced, the clock and zone, the exact-alarm
 * switch (LLP 1133.030 D9). */
export const RECEIVER_ACTIONS = new Set([
  'android.intent.action.BOOT_COMPLETED',
  'android.intent.action.LOCKED_BOOT_COMPLETED',
  'android.intent.action.TIME_SET',
  'android.intent.action.TIMEZONE_CHANGED',
  'android.intent.action.MY_PACKAGE_REPLACED',
  'android.app.action.SCHEDULE_EXACT_ALARM_PERMISSION_STATE_CHANGED',
]);

/** The foreground service types a package's service may declare. */
export const SERVICE_TYPES = new Set(['systemExempted']);

/** The packages a fragment may make visible (`<queries>`): Health Connect's provider on Android 13 and earlier (LLP
 * 1133.026 D19 item 3). */
export const QUERY_PACKAGES = new Set(['com.google.android.apps.healthdata']);

/** The service intent actions a fragment may make visible (`<queries>`): speech recognition and synthesis on apps
 * targeting API 30 and later (LLP 1133.012 D12 item 3). */
export const QUERY_ACTIONS = new Set(['android.speech.RecognitionService', 'android.intent.action.TTS_SERVICE']);

/** The activity intents, by action and data scheme, a fragment may make visible: the mail and message composers'
 * (LLP 1133.024 D22 item 6), whether one resolves being `composeStatus`. */
export const QUERY_SCHEMES = { 'android.intent.action.SENDTO': new Set(['mailto', 'smsto']) };

/** The services another app binds: each binder's permission, and the actions its intent may name (LLP 1133.034 D5). */
export const SERVICE_BINDS = new Map([
  ['com.google.android.gms.permission.BIND_PAYMENTS_CALLBACK_SERVICE', new Set(['com.google.android.gms.wallet.callback.PAYMENT_DATA_CALLBACKS'])],
]);

/** The services a library starts from its own receiver in the app's process, by the one action each answers, with the
 * library's providers the build removes from the merged manifest (`tools:node="remove"`). Firebase's messaging client
 * starts its `FirebaseMessagingService` from `FirebaseInstanceIdReceiver`, and its `FirebaseInitProvider` would
 * initialize Firebase before `Application.onCreate` at every process start; the package initializes it at first use
 * instead (LLP 1133.004.000 D6). */
export const LIBRARY_SERVICES = new Map([['com.google.firebase.MESSAGING_EVENT', ['com.google.firebase.provider.FirebaseInitProvider']]]);

/** The `<meta-data>` a package may declare, by name, with its one value or resource (LLP 1133.034 D13: Google Pay's
 * switch; Firebase's messaging client off at launch and using registration tokens for Expo's push service, and
 * the channel and small icon it gives a notification message shown while the app is in the background, which are the
 * app's default channel and its `exact_notification` drawable, `android-channels.mjs`'). */
export const METADATA = new Map([
  ['com.google.android.gms.wallet.api.enabled', { value: 'true' }],
  ['firebase_messaging_auto_init_enabled', { value: 'false' }],
  ['firebase_messaging_installation_id_enabled', { value: 'false' }],
  ['com.google.firebase.messaging.default_notification_channel_id', { value: 'default' }],
  ['com.google.firebase.messaging.default_notification_icon', { resource: '@drawable/exact_notification' }],
]);

/** The actions an activity may answer: Health Connect's request to show the app's privacy policy. */
export const ACTIVITY_ACTIONS = new Set(['androidx.health.ACTION_SHOW_PERMISSIONS_RATIONALE']);

/** An alias's action and category: Android 14's permission-usage entry for Health (LLP 1133.026 D19 item 3). */
export const ALIAS_ACTIONS = new Set(['android.intent.action.VIEW_PERMISSION_USAGE']);
export const ALIAS_CATEGORIES = new Set(['android.intent.category.HEALTH_PERMISSIONS']);

/** The normal permissions a fragment may declare (LLP 1133.031 D12 item 1): `protectionLevel="normal"` in the
 * platform's own manifest, granted at install with no prompt. */
export const NORMAL_PERMISSIONS = new Set(['android.permission.ACCESS_NETWORK_STATE', 'android.permission.RUN_USER_INITIATED_JOBS', 'android.permission.FOREGROUND_SERVICE', 'android.permission.FOREGROUND_SERVICE_DATA_SYNC']);

/** The normal permissions every bound implementation's fragment declares, which the build adds as `<uses-permission>`
 * beside the grants' (`-Ppermissions`). Throws, naming the package, for any permission not on the list. */
export function fragmentPermissions(bound) {
  const names = new Set();
  for (const impl of bound) {
    const list = impl.manifest?.permissions;
    if (list == null) continue;
    const refuse = (what) => { throw new Error(`${impl.extension}'s Android implementation: its manifest fragment ${what} (LLP 1133.031 D12 item 1: normal permissions only; a runtime permission comes from a grant line)`); };
    if (!Array.isArray(list)) refuse('has permissions that are not a list');
    for (const p of list) {
      if (!NORMAL_PERMISSIONS.has(p)) refuse(`asks for ${JSON.stringify(p)}, which is not on the list`);
      names.add(p);
    }
  }
  return [...names];
}

const FIELDS = {
  receivers: new Set(['name', 'directBootAware', 'actions']),
  services: new Set(['name', 'directBootAware', 'foregroundServiceType', 'permission', 'actions']),
  queries: new Set(['package', 'action', 'scheme']),
  activities: new Set(['name', 'actions']),
  aliases: new Set(['name', 'targetActivity', 'action', 'category']),
  metadata: new Set(['name', 'value', 'resource']),
};

const ONE = { receivers: 'receiver', services: 'service', queries: 'query', activities: 'activity', aliases: 'alias', metadata: 'meta-data' };

const escape = (s) => s.replaceAll('&', '&amp;').replaceAll('"', '&quot;').replaceAll('<', '&lt;').replaceAll('>', '&gt;');

/** The `<meta-data>` names a package's configuration key may fill (LLP 1133.021 §9 item 7), for a vendor SDK that reads
 * its key from the manifest by its own name: Google's Maps SDK, `com.google.android.geo.API_KEY` ([set
 * up](https://developers.google.com/maps/documentation/android-sdk/config)). An interface opts a key in with
 * `"metadata": "<name>"` in its `config.android` declaration. */
export const CONFIG_METADATA = new Set(['com.google.android.geo.API_KEY']);

/** The `<meta-data>` lines `configs` (`packageConfigs`' entries) fill. Throws, naming the package, for a name not on
 * [CONFIG_METADATA] and for a name two packages fill. */
export function configMetaData(configs) {
  const lines = [], filled = new Map();
  for (const { name, declared, values } of configs) for (const [key, d] of Object.entries(declared)) {
    if (d.metadata === undefined) continue;
    if (!CONFIG_METADATA.has(d.metadata)) throw new Error(`${name}'s exact.json: its android configuration key ${key} fills <meta-data> ${JSON.stringify(d.metadata)}, which is not on the list (scripts/android-manifest.mjs CONFIG_METADATA)`);
    if (filled.has(d.metadata)) throw new Error(`${name} and ${filled.get(d.metadata)} both fill <meta-data> ${d.metadata}; an app binds one of them`);
    filled.set(d.metadata, name);
    if (values[key] !== undefined) lines.push(`        <meta-data android:name="${d.metadata}" android:value="${escape(String(values[key]))}" />`);
  }
  return lines;
}

/** `fragment` (from [manifestFragment]) with `meta` lines inside its `<application>`, made if it has none. */
export function withMetaData(fragment, meta) {
  if (!meta.length) return fragment;
  if (fragment.includes('    <application>\n')) return fragment.replace('    <application>\n', `    <application>\n${meta.join('\n')}\n`);
  return [fragment, '    <application>', ...meta, '    </application>'].filter(Boolean).join('\n');
}

function addQueries(queries, list, refuse) {
  if (!Array.isArray(list)) refuse('has queries that is not a list');
  for (const c of list) {
    for (const key of Object.keys(c ?? {})) if (!FIELDS.queries.has(key)) refuse(`gives a query "${key}", which the list does not allow`);
    const keys = Object.keys(c ?? {}).sort().join();
    if (c == null || typeof c !== 'object' || Array.isArray(c) || !['package', 'action', 'action,scheme'].includes(keys)) refuse('gives a query without exactly one of package or action (with a scheme or not)');
    if (Object.hasOwn(c, 'package')) {
      if (!QUERY_PACKAGES.has(c.package)) refuse(`gives a query the package ${JSON.stringify(c.package)}, which is not on the list`);
      queries.add(`        <package android:name="${c.package}" />`);
    } else if (Object.hasOwn(c, 'scheme')) {
      if (typeof c.action !== 'string' || !Object.hasOwn(QUERY_SCHEMES, c.action) || !QUERY_SCHEMES[c.action].has(c.scheme)) refuse(`gives a query the action ${JSON.stringify(c.action)} with the scheme ${JSON.stringify(c.scheme)}, which is not on the list`);
      queries.add(`        <intent><action android:name="${c.action}" /><data android:scheme="${c.scheme}" /></intent>`);
    } else {
      if (!QUERY_ACTIONS.has(c.action)) refuse(`gives a query the action ${JSON.stringify(c.action)}, which is not on the list`);
      queries.add(`        <intent><action android:name="${c.action}" /></intent>`);
    }
  }
}

/** Merge the app's and bound implementations' visibility queries with the packages' application entries.
 * A refused entry names its source. A rationale activity carries the app's privacy policy as meta-data. */
export function manifestFragment(bound, { privacyPolicy = null, appQueries = [] } = {}) {
  const lines = [], queries = new Set(), names = new Set(), removed = new Set();
  addQueries(queries, appQueries, (what) => { throw new Error(`app.json host.android.queries: ${what} (LLP 1133.030 D9's allow-list)`); });
  let activities = 0;
  for (const impl of bound) {
    const fragment = impl.manifest;
    if (fragment == null) continue;
    const who = `${impl.extension}'s Android implementation`, refuse = (what) => { throw new Error(`${who}: its manifest fragment ${what} (LLP 1133.030 D9's allow-list)`); };
    if (typeof fragment !== 'object' || Array.isArray(fragment)) refuse('is not an object');
    // A component is the package's own class: one in the Java package of its `class`.
    const own = impl.className.split('.').slice(0, -1).join('.'), mine = new Set();
    const component = (kind, c) => {
      if (typeof c.name !== 'string' || !/^[A-Za-z_]\w*(\.[A-Za-z_]\w*)+$/.test(c.name) || !c.name.startsWith(`${own}.`)) refuse(`names ${JSON.stringify(c.name)}, not a class in ${own}`);
      if (names.has(c.name)) refuse(`declares ${c.name} twice`);
      names.add(c.name);
    };
    const listed = (set, value, what) => { if (!set.has(value)) refuse(`gives ${what} ${JSON.stringify(value)}, which is not on the list`); };
    // Aliases name an activity, so activities come first whatever the object's order.
    const kinds = Object.keys(fragment).sort((a, b) => (a === 'aliases') - (b === 'aliases'));
    for (const kind of kinds) {
      const list = fragment[kind];
      if (kind === 'queries') { addQueries(queries, list, refuse); continue; }
      if (kind === 'permissions') continue; // fragmentPermissions'
      if (!FIELDS[kind]) refuse(`names "${kind}"; only ${[...Object.keys(FIELDS), 'permissions'].join(', ')} are allowed`);
      if (!Array.isArray(list)) refuse(`has ${kind} that is not a list`);
      for (const c of list) {
        for (const key of Object.keys(c ?? {})) if (!FIELDS[kind].has(key)) refuse(`gives a ${ONE[kind]} "${key}", which the list does not allow`);
        if (kind === 'metadata') {
          const field = Object.hasOwn(c ?? {}, 'resource') ? 'resource' : 'value';
          if (c == null || typeof c !== 'object' || Array.isArray(c) || typeof c.name !== 'string' || typeof c[field] !== 'string' || (field === 'resource' && Object.hasOwn(c, 'value'))) refuse('gives a meta-data without a string name and one string value or resource');
          if (!METADATA.has(c.name) || METADATA.get(c.name)[field] !== c[field]) refuse(`gives the meta-data ${JSON.stringify(c.name)} the ${field} ${JSON.stringify(c[field])}, which is not on the list`);
          if (names.has(c.name)) refuse(`declares ${c.name} twice`);
          names.add(c.name);
          lines.push(`        <meta-data android:name="${escape(c.name)}" android:${field}="${escape(c[field])}" />`);
          continue;
        }
        if (kind === 'services' && c.name === 'androidx.work.impl.foreground.SystemForegroundService') {
          if (c.foregroundServiceType !== 'dataSync' || c.directBootAware !== undefined || c.permission !== undefined || c.actions !== undefined) refuse('gives WorkManager a service declaration other than dataSync');
          if (!names.has(c.name)) lines.push(`        <service android:name="${c.name}" android:exported="false" android:foregroundServiceType="dataSync" tools:node="merge" />`);
          names.add(c.name);
          continue;
        }
        component(kind, c);
        if (kind === 'services' && c.permission === 'android.permission.BIND_JOB_SERVICE') {
          if (c.foregroundServiceType !== undefined || c.directBootAware !== undefined || c.actions !== undefined) refuse('gives a BIND_JOB_SERVICE service a foreground type, direct boot or actions');
          lines.push(`        <service android:name="${c.name}" android:exported="false" android:permission="android.permission.BIND_JOB_SERVICE" />`);
          continue;
        }
        if (kind === 'activities') {
          const actions = c.actions ?? [];
          if (!Array.isArray(actions) || !actions.length) refuse(`gives ${c.name} no actions; an activity is only for one the system starts`);
          for (const a of actions) listed(ACTIVITY_ACTIONS, a, `${c.name} the action`);
          mine.add(c.name);
          activities++;
          lines.push(`        <activity android:name="${c.name}" android:exported="true" android:theme="@android:style/Theme.NoDisplay">`, '            <intent-filter>',
            ...actions.map((a) => `                <action android:name="${a}" />`), '            </intent-filter>', '        </activity>');
          continue;
        }
        if (kind === 'aliases') {
          if (!mine.has(c.targetActivity)) refuse(`gives ${c.name} the target ${JSON.stringify(c.targetActivity)}, not an activity it declares`);
          listed(ALIAS_ACTIONS, c.action, `${c.name} the action`);
          listed(ALIAS_CATEGORIES, c.category, `${c.name} the category`);
          lines.push(`        <activity-alias android:name="${c.name}" android:exported="true" android:targetActivity="${c.targetActivity}" android:permission="android.permission.START_VIEW_PERMISSION_USAGE">`,
            '            <intent-filter>', `                <action android:name="${c.action}" />`, `                <category android:name="${c.category}" />`, '            </intent-filter>', '        </activity-alias>');
          continue;
        }
        if (c.directBootAware !== undefined && typeof c.directBootAware !== 'boolean') refuse(`gives ${c.name} a directBootAware that is not true or false`);
        const boot = c.directBootAware ? ' android:directBootAware="true"' : '';
        if (kind === 'receivers') {
          const actions = c.actions ?? [];
          if (!Array.isArray(actions)) refuse(`gives ${c.name} actions that are not a list`);
          for (const a of actions) listed(RECEIVER_ACTIONS, a, `${c.name} the action`);
          if (!actions.length) lines.push(`        <receiver android:name="${c.name}" android:exported="false"${boot} />`);
          else lines.push(`        <receiver android:name="${c.name}" android:exported="false"${boot}>`, '            <intent-filter>',
            ...actions.map((a) => `                <action android:name="${a}" />`), '            </intent-filter>', '        </receiver>');
        } else {
          const type = c.foregroundServiceType;
          if (type !== undefined) listed(SERVICE_TYPES, type, `${c.name} the foreground service type`);
          if (c.permission === undefined && c.actions === undefined) {
            lines.push(`        <service android:name="${c.name}" android:exported="false"${boot}${type ? ` android:foregroundServiceType="${type}"` : ''} />`);
            continue;
          }
          if (c.permission === undefined) {
            if (!Array.isArray(c.actions) || c.actions.length !== 1 || !LIBRARY_SERVICES.has(c.actions[0])) refuse(`gives ${c.name} actions without a permission that are not one library's action on the list`);
            if (type !== undefined || c.directBootAware) refuse(`gives the library's service ${c.name} a foreground type or direct boot`);
            for (const provider of LIBRARY_SERVICES.get(c.actions[0])) removed.add(provider);
            lines.push(`        <service android:name="${c.name}" android:exported="false">`, '            <intent-filter>',
              `                <action android:name="${c.actions[0]}" />`, '            </intent-filter>', '        </service>');
            continue;
          }
          const actions = SERVICE_BINDS.get(c.permission);
          if (!actions) refuse(`gives ${c.name} the permission ${JSON.stringify(c.permission)}, which is not on the list`);
          if (!Array.isArray(c.actions) || !c.actions.length || c.actions.some((a) => !actions.has(a))) refuse(`gives ${c.name} actions that ${c.permission} does not bind`);
          if (type !== undefined || c.directBootAware) refuse(`gives the bound service ${c.name} a foreground type or direct boot`);
          lines.push(`        <service android:name="${c.name}" android:exported="true" android:permission="${c.permission}">`, '            <intent-filter>',
            ...c.actions.map((a) => `                <action android:name="${a}" />`), '            </intent-filter>', '        </service>');
        }
      }
    }
  }
  for (const provider of removed) lines.push(`        <provider android:name="${provider}" tools:node="remove" />`);
  if (activities && privacyPolicy) lines.unshift(`        <meta-data android:name="exact.privacy_policy" android:value="${escape(privacyPolicy)}" />`);
  return [
    ...(queries.size ? ['    <queries>', ...queries, '    </queries>'] : []),
    ...(lines.length ? ['    <application>', ...lines, '    </application>'] : []),
  ].join('\n');
}

/** The app's activity as a fragment extends it: `singleTask`, so a link or a share another app sends brings this task
 * forward to `onNewIntent` rather than stacking a second reader in the sender's task. */
const MAIN_ACTIVITY = '        <activity android:name="dev.exact.app.MainActivity" android:launchMode="singleTask" tools:replace="android:launchMode">';

/** `fragment` with `lines` (intent filters) on the app's activity and `meta` on the application: a share target, auth
 * callbacks and links extend one activity element instead of duplicating it. */
function onMainActivity(fragment, lines, meta = []) {
  if (fragment.includes(MAIN_ACTIVITY)) return withMetaData(fragment.replace(MAIN_ACTIVITY, [MAIN_ACTIVITY, ...lines].join('\n')), meta);
  return withMetaData(fragment, [...meta, MAIN_ACTIVITY, ...lines, '        </activity>']);
}

/** A browsable VIEW filter for `scheme`: links in other apps and browsers that use it start this activity. */
const viewFilter = (scheme) => [
  '            <intent-filter>',
  '                <action android:name="android.intent.action.VIEW" />',
  '                <category android:name="android.intent.category.DEFAULT" />',
  '                <category android:name="android.intent.category.BROWSABLE" />',
  `                <data android:scheme="${escape(scheme)}" />`,
  '            </intent-filter>',
];

/** The handlers an Android build registers (LLP 1038 D8): each custom scheme of `protocol_handlers`, lowercased, as
 * Android matches schemes case-sensitively; a `distribution_only` one only in a production build, as an Apple archive;
 * never `http` and `https`, which make a browser (Android is not one, docs/reference.md). */
export const androidProtocolHandlers = (manifest, distributed = false) => (manifest?.protocol_handlers ?? [])
  .filter((h) => !['http', 'https'].includes(h.protocol.toLowerCase()) && (!h.distribution_only || distributed))
  .map(({ protocol, url }) => ({ protocol: protocol.toLowerCase(), url }));

/** Both declarations register browsable VIEW filters. Only protocol handlers supply templates to the reader.
 * A URL with no template reaches `location_of` directly (LLP 1038 D8). */
export function withProtocolHandlers(fragment, manifest, distributed = false) {
  const handlers = androidProtocolHandlers(manifest, distributed), urlSchemes = manifest?.host?.android?.urlSchemes ?? [];
  const schemes = new Set([...handlers.map((h) => h.protocol), ...urlSchemes.flatMap((s) => [s, s.toLowerCase()])]);
  if (!schemes.size) return fragment;
  const metadata = handlers.length ? [`        <meta-data android:name="dev.exact.protocol_handlers" android:value="${escape(JSON.stringify(handlers))}" />`] : [];
  return onMainActivity(fragment, [...schemes].flatMap(viewFilter), metadata);
}

/** The App Links on the app's activity (LLP 1038 D8): for each claimed host (`androidLinkFilters`), an `autoVerify`
 * https filter, which Android opens in the app once the host's `/.well-known/assetlinks.json` names it, the link's path
 * its location. From Android 15 (API 35) `<uri-relative-filter-group android:allow="false">` leaves each excluded
 * path to the browser; earlier releases ignore the element (Android, "<uri-relative-filter-group>"). The groups are
 * ordered, the first that matches deciding, and a filter with groups matches only a URI a group allows (an emulator on
 * Android 17 opens nothing without it), so a group allowing the scope comes last. The filter's own `pathPrefix`, where
 * `androidLinkFilters` gives one, narrows it on every release. */
export function withAppLinks(fragment, filters = []) {
  if (!filters.length) return fragment;
  const group = (allow, data) => [`                <uri-relative-filter-group${allow ? '' : ' android:allow="false"'}>`, `                    <data ${data} />`, '                </uri-relative-filter-group>'];
  const pathData = (p) => p.endsWith(EXACT_PATHS) ? `android:pathPrefix="${escape(p)}"` : `android:path="${escape(p)}"`;
  const lines = filters.flatMap(({ host, pathPrefix, exclude, allow }) => [
    '            <intent-filter android:autoVerify="true">',
    '                <action android:name="android.intent.action.VIEW" />',
    '                <category android:name="android.intent.category.DEFAULT" />',
    '                <category android:name="android.intent.category.BROWSABLE" />',
    '                <data android:scheme="https" />',
    `                <data android:host="${escape(host)}" />`,
    ...(pathPrefix ? [`                <data android:pathPrefix="${escape(pathPrefix)}" />`] : []),
    ...(exclude.length ? [...exclude.flatMap((p) => group(false, pathData(p))), ...group(true, `android:pathPrefix="${escape(allow)}"`)] : []),
    '            </intent-filter>',
  ]);
  return onMainActivity(fragment, lines);
}

/** HTTPS auth callbacks have their own exact-path App Link for browsers without Auth Tab. Custom schemes share
 * their protocol handler's filter. The runner checks the full endpoint and state; HTTP loopback is not registered. */
export function withAuthCallbacks(fragment, callbacks = []) {
  const endpoints = androidAuthCallbacks(callbacks);
  const schemes = [...new Set(endpoints.filter(callback => new URL(callback).protocol !== 'https:').map(callback => new URL(callback).protocol.slice(0, -1)))];
  if (!endpoints.length) return fragment;
  // A scheme a protocol handler already filters is not filtered twice.
  const filters = schemes.filter((scheme) => !fragment.includes(viewFilter(scheme).join('\n'))).flatMap(viewFilter);
  for (const callback of endpoints.filter(callback => new URL(callback).protocol === 'https:')) {
    const url = new URL(callback);
    filters.push('            <intent-filter android:autoVerify="true">',
      '                <action android:name="android.intent.action.VIEW" />',
      '                <category android:name="android.intent.category.DEFAULT" />',
      '                <category android:name="android.intent.category.BROWSABLE" />',
      `                <data android:scheme="https" android:host="${escape(url.hostname)}" android:path="${escape(decodeURIComponent(url.pathname))}" />`,
      '            </intent-filter>');
  }
  // A hostless scheme:/path cannot be narrowed by android:path: Android ignores paths without a host.
  const app = onMainActivity(fragment, filters, [`        <meta-data android:name="dev.exact.auth.callbacks" android:value="${escape(JSON.stringify(endpoints))}" />`]);
  const queries = [
    '        <intent><action android:name="android.support.customtabs.action.CustomTabsService" /></intent>',
    '        <intent><action android:name="android.intent.action.VIEW" /><category android:name="android.intent.category.BROWSABLE" /><data android:scheme="https" /></intent>',
  ].join('\n');
  if (app.includes('    </queries>')) return app.replace('    </queries>', `${queries}\n    </queries>`);
  return `${app}\n    <queries>\n${queries}\n    </queries>`;
}
