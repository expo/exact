import type { ExpoPushApp, ExpoPushOptions, ExpoPushSubscription } from './expo.d.ts';
export type { ExpoPushApp, ExpoPushOptions, ExpoPushSubscription } from './expo.d.ts';

/** Exchange the current APNs or FCM token with Expo. Call again when pushSubscription changes; send the result to your server. */
export async function getExpoPushTokenAsync(subscription: ExpoPushSubscription, app: ExpoPushApp, options: ExpoPushOptions = {}): Promise<{ type: 'expo'; data: string }> {
  const device = deviceOf(subscription, options);
  const projectId = app.extra?.eas?.projectId;
  if (!device) throw new TypeError('Expo push needs an APNs device token, or an Android FCM token and installationId');
  if (!projectId) throw new TypeError('app.json extra.eas.projectId is required for Expo push');
  if (!device.appId) throw new TypeError(`${device.platform} push subscription must name its application id`);
  if (!device.installation) throw new TypeError(`${device.platform} push subscription must name its installationId`);
  if (typeof device.development !== 'boolean') {
    throw new TypeError('Expo push needs options.development for an APNs token: true when the build registers with the APNs sandbox');
  }
  const response = await fetch('https://exp.host/--/api/v2/push/getExpoPushToken', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ type: device.type, deviceId: await expoDeviceId(device.installation), development: device.development,
      appId: device.appId, deviceToken: device.token, projectId }),
  });
  if (!response.ok) {
    const detail = (await response.text().catch(() => '')).slice(0, 500);
    throw new Error(`Expo push token exchange failed (${response.status})${detail ? ` ${detail}` : ''}`);
  }
  const result = await response.json();
  if (typeof result?.data?.expoPushToken !== 'string' || !result.data.expoPushToken) {
    throw new Error('Expo push token exchange returned no expoPushToken');
  }
  return { type: 'expo', data: result.data.expoPushToken };
}

/**
 * What Expo's exchange takes from a platform's credential. `installation` seeds Expo's per-installation UUID, so it
 * stays the same across token rotation: Firebase's case-sensitive installation ID, or the UUID the Apple package keeps
 * in the Keychain for the installation.
 */
function deviceOf({ apple, android }: ExpoPushSubscription, options: ExpoPushOptions) {
  if (apple?.deviceToken) {
    return { type: 'apns', platform: 'Apple', token: apple.deviceToken, installation: apple.installationId, appId: apple.appId, development: options.development };
  }
  if (android?.token && android.installationId) {
    return { type: 'fcm', platform: 'Android', token: android.token, installation: android.installationId, appId: android.appId, development: false };
  }
  return null;
}

async function expoDeviceId(installation: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(installation));
  const bytes = new Uint8Array(digest, 0, 16);
  bytes[6] = (bytes[6]! & 0x0f) | 0x40;
  bytes[8] = (bytes[8]! & 0x3f) | 0x80;
  const hex = Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
