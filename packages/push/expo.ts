import type { ExpoPushApp, ExpoPushSubscription } from './expo.d.ts';
export type { ExpoPushApp, ExpoPushSubscription } from './expo.d.ts';

/** Exchange the current FCM token with Expo. Call again when pushSubscription changes; send the result to your server. */
export async function getExpoPushTokenAsync(subscription: ExpoPushSubscription, app: ExpoPushApp): Promise<{ type: 'expo'; data: string }> {
  const android = subscription.android;
  const projectId = app.extra?.eas?.projectId;
  if (!android?.token || !android.installationId) throw new TypeError('Expo push needs an Android FCM token and installationId');
  if (!projectId) throw new TypeError('app.json extra.eas.projectId is required for Expo push');
  if (!android.appId) throw new TypeError('Android push subscription must name its application id');
  // Match expo-notifications/src/getExpoPushTokenAsync.ts. The installation ID is stable across process starts.
  const response = await fetch('https://exp.host/--/api/v2/push/getExpoPushToken', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ type: 'fcm', deviceId: android.installationId.toLowerCase(), development: false,
      appId: android.appId, deviceToken: android.token, projectId }),
  });
  if (!response.ok) throw new Error(`Expo push token exchange failed (${response.status})`);
  const result = await response.json();
  if (typeof result?.data?.expoPushToken !== 'string' || !result.data.expoPushToken) {
    throw new Error('Expo push token exchange returned no expoPushToken');
  }
  return { type: 'expo', data: result.data.expoPushToken };
}
