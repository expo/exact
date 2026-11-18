import type { ExpoPushApp, ExpoPushSubscription } from './expo.d.ts';
export type { ExpoPushApp, ExpoPushSubscription } from './expo.d.ts';

/** Exchange the current FCM token with Expo. Call again when pushSubscription changes; send the result to your server. */
export async function getExpoPushTokenAsync(subscription: ExpoPushSubscription, app: ExpoPushApp): Promise<{ type: 'expo'; data: string }> {
  const android = subscription.android;
  const projectId = app.extra?.eas?.projectId;
  if (!android?.token || !android.installationId) throw new TypeError('Expo push needs an Android FCM token and installationId');
  if (!projectId) throw new TypeError('app.json extra.eas.projectId is required for Expo push');
  if (!android.appId) throw new TypeError('Android push subscription must name its application id');
  const response = await fetch('https://exp.host/--/api/v2/push/getExpoPushToken', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ type: 'fcm', deviceId: await expoDeviceId(android.installationId), development: false,
      appId: android.appId, deviceToken: android.token, projectId }),
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

// Expo uses a per-installation UUID. Deriving it from the case-sensitive Firebase ID needs no separate storage.
async function expoDeviceId(installationId: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(installationId));
  const bytes = new Uint8Array(digest, 0, 16);
  bytes[6] = (bytes[6]! & 0x0f) | 0x40;
  bytes[8] = (bytes[8]! & 0x3f) | 0x80;
  const hex = Array.from(bytes, byte => byte.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
