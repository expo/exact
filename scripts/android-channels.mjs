// An Android app's notification resources (LLP 1133.004.000 D1, D3, D6), written under `res` for the Gradle shell:
// the channels the bake resolved (`reach.androidChannels`, `bake/src/channels.rs`) as a list the host reads
// (`xml/exact_channels.xml`, `Channels.kt`), their names and descriptions as string resources per strings table,
// their sounds in `res/raw`, the status bar's small icon from the manifest's `monochrome` icon, and Firebase's
// options from `google-services.json` for an app that pushes.
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, extname, resolve } from 'node:path';
import { pickIcon, placeRgba, readPng, resize } from './app-icons.mjs';
import { encodePng } from './png.mjs';

/** The small icon's densities: 24 dp at each, as Material sizes a status bar icon. */
const DENSITIES = { mdpi: 24, hdpi: 36, xhdpi: 48, xxhdpi: 72, xxxhdpi: 96 };

/** The resource directory a strings table's BCP 47 tag goes to: the base table is the default (`values`), a bare
 * language its own (`values-es`), and anything more Android's BCP 47 form (`values-b+pt+BR`). */
export function valuesDirectory(tag, base) {
  if (tag === base) return 'values';
  return /^[a-z]{2,3}$/.test(tag) ? `values-${tag}` : `values-b+${tag.split('-').join('+')}`;
}

/** A string resource's text: XML's escapes, then the resource compiler's own (a backslash, quotes, `@` or `?`
 * leading). */
export const resourceText = (v) => v.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('\\', '\\\\')
  .replaceAll("'", "\\'").replaceAll('"', '\\"').replace(/^([@?])/, '\\$1');

/** The small icon (D3): the `monochrome` glyph, its transparent margin trimmed, fitted into Material's 22 dp live
 * area of a 24 dp square at each density. Null when the manifest has none. */
export function notificationIcon(manifest, dir) {
  const glyph = pickIcon(manifest, dir, { formats: ['png'], native: false, purposes: ['monochrome'], px: 96, glyph: true, name: "Android's notification icon" });
  if (!glyph) return null;
  const image = readPng(glyph.path);
  let [left, top, right, bottom] = [image.width, image.height, -1, -1];
  for (let y = 0; y < image.height; y++) for (let x = 0; x < image.width; x++) {
    if (image.data[(y * image.width + x) * 4 + 3] === 0) continue;
    left = Math.min(left, x); right = Math.max(right, x); top = Math.min(top, y); bottom = Math.max(bottom, y);
  }
  if (right < 0) return null;
  const [w, h] = [right - left + 1, bottom - top + 1], trimmed = { width: w, height: h, data: new Uint8Array(w * h * 4) };
  for (let y = 0; y < h; y++) trimmed.data.set(image.data.subarray(((top + y) * image.width + left) * 4, ((top + y) * image.width + left + w) * 4), y * w * 4);
  return Object.fromEntries(Object.entries(DENSITIES).map(([density, px]) => {
    // Shrunk only: a glyph smaller than the live area keeps its size.
    const live = (px * 22) / 24, scale = Math.min(live / w, live / h, 1);
    const [sw, sh] = [Math.max(1, Math.round(w * scale)), Math.max(1, Math.round(h * scale))];
    // The glyph's alpha drawn white: Android draws a small icon by its alpha alone.
    const placed = placeRgba(px, px, resize(trimmed, sw, sh), Math.round((px - sw) / 2), Math.round((px - sh) / 2));
    for (let i = 0; i < placed.data.length; i += 4) placed.data.fill(255, i, i + 3);
    return [density, placed];
  }));
}

/** Firebase's options for the client in `google-services.json` whose `package_name` is `id` (D6), as the string
 * resources the Google Services Gradle plugin would write, name to text
 * ([the plugin's processing](https://developers.google.com/android/guides/google-services-plugin)), read without
 * the plugin. `path` is `packages["@exact/push"].android.googleServices`, relative to the app, tried before the file
 * beside app.json. Null when neither yields that client. */
export function googleServices(dir, id, path) {
  for (const file of new Set([path, 'google-services.json'].filter((f) => typeof f === 'string'))) {
    let json;
    try { json = JSON.parse(readFileSync(resolve(dir, file), 'utf8')); } catch { continue; }
    const client = (Array.isArray(json?.client) ? json.client : []).find((c) => c?.client_info?.android_client_info?.package_name === id);
    if (typeof client?.client_info?.mobilesdk_app_id !== 'string') continue;
    const project = json.project_info ?? {};
    return Object.fromEntries(Object.entries({ google_app_id: client.client_info.mobilesdk_app_id, gcm_defaultSenderId: project.project_number,
      google_api_key: client.api_key?.[0]?.current_key, project_id: project.project_id, google_storage_bucket: project.storage_bucket })
      .filter(([, v]) => typeof v === 'string' && v));
  }
  return null;
}

/** Writes the notification resources under `res` (emptied first) and returns what the build says: `null` when the
 * app grants no notifications (`channels` absent), else `{ icon }`, whether the manifest gave a small icon. `channels`
 * is the bake's `reach.androidChannels`; `locales` and `base` its strings tables'; `firebase` is `googleServices`'
 * values, for an app that pushes. Without a `monochrome` icon, `exact_notification` is the platform's generic glyph,
 * so the drawable is there for every app that grants notifications and a manifest fragment can name it (Firebase's
 * `default_notification_icon`). `activities` is the bake's `reach.androidActivities`, each `{ kind, android }`. */
export function androidNotificationResources(manifest, dir, res, { channels, locales, base, firebase = null, activities = null }) {
  rmSync(res, { recursive: true, force: true });
  if (!Array.isArray(channels)) return null;
  const write = (path, text) => { mkdirSync(dirname(resolve(res, path)), { recursive: true }); writeFileSync(resolve(res, path), text); };
  const strings = new Map();
  const items = channels.map((c) => {
    const name = `exact_channel_${c.id}`;
    for (const [field, texts] of [['name', c.name], ['description', c.description]]) {
      if (!texts) continue;
      for (const locale of locales ?? [base]) {
        const folder = valuesDirectory(locale, base);
        if (!strings.has(folder)) strings.set(folder, []);
        strings.get(folder).push(`    <string name="${name}_${field}">${resourceText(texts[locale] ?? texts[base])}</string>`);
      }
    }
    if (c.sound) {
      const raw = resolve(res, 'raw', `${name}${extname(c.sound).toLowerCase()}`);
      mkdirSync(dirname(raw), { recursive: true });
      copyFileSync(resolve(dir, c.sound), raw);
    }
    return `    <channel id="${c.id}" importance="${c.importance}" name="@string/${name}_name"` +
      (c.description ? ` description="@string/${name}_description"` : '') + (c.sound ? ` sound="@raw/${name}"` : '') + ' />';
  });
  for (const [folder, lines] of strings) write(`${folder}/exact_channels.xml`, `<resources>\n${lines.join('\n')}\n</resources>\n`);
  write('xml/exact_channels.xml', `<channels>\n${items.map((l) => `${l}\n`).join('')}</channels>\n`);
  // The live activities Android shows as ongoing notifications, which the reader's `Activities` reads (LLP 1126 D13).
  if (Array.isArray(activities) && activities.length) write('raw/exact_activities.json', `${JSON.stringify(activities)}\n`);
  if (firebase) write('values/exact_google_services.xml', `<resources>\n${Object.entries(firebase)
    .map(([name, text]) => `    <string name="${name}" translatable="false">${resourceText(text)}</string>\n`).join('')}</resources>\n`);
  const icon = notificationIcon(manifest, dir);
  if (icon) for (const [density, image] of Object.entries(icon)) {
    mkdirSync(resolve(res, `drawable-${density}`), { recursive: true });
    writeFileSync(resolve(res, `drawable-${density}`, 'exact_notification.png'), encodePng(image));
  }
  else write('drawable/exact_notification.xml', '<inset xmlns:android="http://schemas.android.com/apk/res/android" android:drawable="@android:drawable/ic_popup_reminder" />\n');
  return { icon: !!icon };
}

/** The receiver that makes the channels again when the app is updated, with no session (D1), inside the manifest
 * fragment's `<application>`. */
export const CHANNELS_RECEIVER = [
  '        <receiver android:name="dev.exact.canvas.Channels$Replaced" android:exported="false">',
  '            <intent-filter>', '                <action android:name="android.intent.action.MY_PACKAGE_REPLACED" />', '            </intent-filter>',
  '        </receiver>',
];
