import { afterEach, expect, mock, test } from 'bun:test';
import { getExpoPushTokenAsync } from '../expo';

const original = globalThis.fetch;
afterEach(() => { globalThis.fetch = original; });
const app = { app: { id: 'dev.guard' }, extra: { eas: { projectId: 'project-id' } } };
const subscription = { android: { installationId: 'Installation-ID', token: 'real-FCM-token', appId: 'dev.guard' } };

test('Expo receives its FCM token exchange request and the app receives its Expo token', async () => {
  const fetch = mock(async () => Response.json({ data: { expoPushToken: 'ExponentPushToken[guard]' } }));
  globalThis.fetch = fetch as unknown as typeof globalThis.fetch;
  expect(await getExpoPushTokenAsync(subscription, app)).toEqual({ type: 'expo', data: 'ExponentPushToken[guard]' });
  expect(fetch).toHaveBeenCalledWith('https://exp.host/--/api/v2/push/getExpoPushToken', {
    method: 'POST', headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ type: 'fcm', deviceId: '7d10ba7e-6150-4625-8182-a8b2a781f8be', development: false,
      appId: 'dev.guard', deviceToken: 'real-FCM-token', projectId: 'project-id' }),
  });
  await getExpoPushTokenAsync({ android: { ...subscription.android, appId: 'dev.guard.android' } }, app);
  expect(JSON.parse((fetch.mock.calls[1] as unknown as [string, RequestInit])[1].body as string).appId).toBe('dev.guard.android');
});

test('Expo deviceId is a stable UUID for the installation, independent of FCM token rotation', async () => {
  const devices: string[] = [];
  globalThis.fetch = mock(async (_url: string, request: RequestInit) => {
    devices.push(JSON.parse(request.body as string).deviceId);
    return Response.json({ data: { expoPushToken: 'ExponentPushToken[guard]' } });
  }) as unknown as typeof fetch;
  const installationId = 'cQ9azN2kTa_FYxgU4R0j1B';
  for (const android of [
    { ...subscription.android, installationId },
    { ...subscription.android, installationId, token: 'rotated-FCM-token' },
    { ...subscription.android, installationId: 'dQ9azN2kTa_FYxgU4R0j1B' },
    { ...subscription.android, installationId: installationId.toLowerCase() },
  ]) await getExpoPushTokenAsync({ android }, app);
  for (const device of devices) expect(device).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
  expect(devices[1]).toBe(devices[0]);
  expect(new Set(devices).size).toBe(3);
});

test('Expo receives the installed package id, including normalization and the selected variant', async () => {
  const fetch = mock(async () => Response.json({ data: { expoPushToken: 'ExponentPushToken[guard]' } }));
  globalThis.fetch = fetch as unknown as typeof globalThis.fetch;
  for (const [manifest, appId] of [
    [{ ...app, app: { id: 'dev.gate-house' } }, 'dev.gate_house'],
    [{ ...app, variants: { dev: { idSuffix: '.dev', name: 'Guard Dev' } } }, 'dev.guard.dev'],
    [{ ...app, host: { android: { id: 'dev.gate-house' } } }, 'dev.gate_house.dev'],
  ] as const) {
    await getExpoPushTokenAsync({ android: { ...subscription.android, appId } }, manifest);
    const request = fetch.mock.calls.at(-1) as unknown as [string, RequestInit];
    expect(JSON.parse(request[1].body as string).appId).toBe(appId);
  }
});

test('missing credentials or project configuration make no request', async () => {
  const fetch = mock();
  globalThis.fetch = fetch as unknown as typeof globalThis.fetch;
  await expect(getExpoPushTokenAsync({}, app)).rejects.toThrow('FCM token');
  await expect(getExpoPushTokenAsync(subscription, { ...app, extra: { eas: { projectId: '' } } })).rejects.toThrow('extra.eas.projectId');
  await expect(getExpoPushTokenAsync({ android: { ...subscription.android, appId: '' } }, app)).rejects.toThrow('application id');
  expect(fetch).not.toHaveBeenCalled();
});

test('network, HTTP and malformed exchange replies are failures', async () => {
  globalThis.fetch = mock(async () => { throw new Error('offline'); }) as unknown as typeof fetch;
  await expect(getExpoPushTokenAsync(subscription, app)).rejects.toThrow('offline');
  for (const response of [new Response('unavailable', { status: 503 }), Response.json({ errors: [{ message: 'invalid project' }] }), Response.json({ data: {} })]) {
    globalThis.fetch = mock(async () => response) as unknown as typeof fetch;
    await expect(getExpoPushTokenAsync(subscription, app)).rejects.toThrow();
  }
});

test('HTTP failures include the status and at most 500 characters of the reply, even when its body fails', async () => {
  const refusal = JSON.stringify({ errors: [{ code: 'VALIDATION_ERROR', message: '"deviceId": Invalid UUID.' }] });
  const unreadable = new Response(new ReadableStream({ start(controller) { controller.error(new Error('broken body')); } }), { status: 502 });
  for (const [response, message] of [
    [new Response(refusal, { status: 400 }), `Expo push token exchange failed (400) ${refusal}`],
    [new Response('x'.repeat(501), { status: 503 }), `Expo push token exchange failed (503) ${'x'.repeat(500)}`],
    [new Response('', { status: 403 }), 'Expo push token exchange failed (403)'],
    [unreadable, 'Expo push token exchange failed (502)'],
  ] as const) {
    globalThis.fetch = mock(async () => response) as unknown as typeof fetch;
    await expect(getExpoPushTokenAsync(subscription, app)).rejects.toMatchObject({ message });
  }
});

const installationId = '6f1d3a52-8c0e-4b7a-9e21-5d4c3b2a1f00';
const apple = { apple: { deviceToken: '0a1b2c3d'.repeat(8), appId: 'dev.guard', installationId } };
const sent = (fetch: ReturnType<typeof mock>, call: number) => JSON.parse((fetch.mock.calls[call] as unknown as [string, RequestInit])[1].body as string);

test('Expo receives its APNs token exchange request for the environment the caller names', async () => {
  const fetch = mock(async () => Response.json({ data: { expoPushToken: 'ExponentPushToken[guard]' } }));
  globalThis.fetch = fetch as unknown as typeof globalThis.fetch;
  expect(await getExpoPushTokenAsync(apple, app, { development: true })).toEqual({ type: 'expo', data: 'ExponentPushToken[guard]' });
  expect(fetch).toHaveBeenCalledWith('https://exp.host/--/api/v2/push/getExpoPushToken', {
    method: 'POST', headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ type: 'apns', deviceId: 'b8cededb-b122-44e7-b117-7bc9ce725af5', development: true,
      appId: 'dev.guard', deviceToken: apple.apple.deviceToken, projectId: 'project-id' }),
  });
  await getExpoPushTokenAsync({ apple: { ...apple.apple, appId: 'dev.guard.dev' } }, app, { development: false });
  expect(sent(fetch, 1)).toMatchObject({ development: false, appId: 'dev.guard.dev', deviceId: 'b8cededb-b122-44e7-b117-7bc9ce725af5' });
  await getExpoPushTokenAsync(subscription, app, { development: true });
  expect(sent(fetch, 2)).toMatchObject({ type: 'fcm', development: false });
});

test('a replacement APNs token is sent for the same Expo deviceId, and another installation has its own', async () => {
  const fetch = mock(async () => Response.json({ data: { expoPushToken: 'ExponentPushToken[guard]' } }));
  globalThis.fetch = fetch as unknown as typeof globalThis.fetch;
  const replacement = '4e5f6071'.repeat(8);
  await getExpoPushTokenAsync(apple, app, { development: false });
  await getExpoPushTokenAsync({ apple: { ...apple.apple, deviceToken: replacement } }, app, { development: false });
  expect(sent(fetch, 1)).toMatchObject({ type: 'apns', deviceToken: replacement, deviceId: sent(fetch, 0).deviceId });
  await getExpoPushTokenAsync({ apple: { ...apple.apple, deviceToken: replacement, installationId: '0d9c8b7a-6f5e-4d3c-8b2a-1f0e9d8c7b6a' } }, app, { development: false });
  expect(sent(fetch, 2).deviceId).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
  expect(sent(fetch, 2).deviceId).not.toBe(sent(fetch, 0).deviceId);
});

test('an APNs exchange without a named environment, bundle id, installation, token or project makes no request', async () => {
  const fetch = mock();
  globalThis.fetch = fetch as unknown as typeof globalThis.fetch;
  await expect(getExpoPushTokenAsync(apple, app)).rejects.toThrow('development');
  await expect(getExpoPushTokenAsync({ apple: { ...apple.apple, appId: '' } }, app, { development: true })).rejects.toThrow('application id');
  await expect(getExpoPushTokenAsync({ apple: { ...apple.apple, installationId: '' } }, app, { development: true })).rejects.toThrow('installationId');
  await expect(getExpoPushTokenAsync({ apple: { ...apple.apple, deviceToken: '' } }, app, { development: true })).rejects.toThrow('APNs device token');
  await expect(getExpoPushTokenAsync(apple, { ...app, extra: { eas: { projectId: '' } } }, { development: true })).rejects.toThrow('extra.eas.projectId');
  expect(fetch).not.toHaveBeenCalled();
});
