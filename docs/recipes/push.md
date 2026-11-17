# Android push through Expo

Add `push` and `notifications` with `bun exact.mjs add push` and
`bun exact.mjs add notifications`. Put the Firebase client's
`google-services.json` beside `app.json`; its package name must match the
Android app id. Configure the same Firebase project's FCM v1 credentials
in the EAS project, as [Expo's setup guide](https://docs.expo.dev/push-notifications/fcm-credentials/)
describes.
Add these fields to `app.json`. The channel's name is a key in the app's
string tables; put `"channel.urgencies": "Urgencies"` in each locale.
The sound is a `.wav`, `.ogg` or `.mp3` asset. A changed importance needs a
new channel id.

```json
{
  "extra": { "eas": { "projectId": "your-EAS-project-UUID" } },
  "host": { "android": { "notificationChannels": [
    { "id": "urgencies", "name": "channel.urgencies", "importance": "high",
      "sound": "assets/urgencynotification.wav" }
  ] } }
}
```

The [complete app example](../reference.md#android-expo-push-example) subscribes
from a press, exchanges each live FCM token with Expo, and handles a data
message by posting on `urgencies`. Its imported `app.json` names the EAS project;
the subscription names the installed package. Grant `net.fetch https://exp.host` and the
backend's origin. Await the handler's work so the wake owns it.

The backend sends the saved Expo token to Expo's send endpoint. The FCM
credentials stay on Expo; no server key belongs in the app. This Bun example
sends one panic alert and checks Expo's ticket:

```ts
const response = await fetch('https://exp.host/--/api/v2/push/send', {
  method: 'POST', headers: { 'content-type': 'application/json' },
  body: JSON.stringify({ to: process.env.EXPO_PUSH_TOKEN,
    title: 'Urgency: Police', body: 'House 12', priority: 'high',
    channelId: 'urgencies', sound: 'default',
    data: { id: 'urgency-42', navigate: '/urgencies' } }),
});
if (!response.ok) throw new Error(`Expo HTTP ${response.status}`);
const ticket = await response.json();
console.log(ticket);
if (ticket.data?.status !== 'ok') throw new Error('Expo refused the push');
```

Check `push/getReceipts` too: a ticket does not prove delivery. The channel supplies
the app sound on Android. `high` is Android's highest used importance;
`IMPORTANCE_MAX` is documented as unused.

A data-only message raises `push` in the live or headless session. Boot,
queueing and handler work share ten seconds. It cannot ask permission or
open UI. A notification message shows directly and raises no `push` event.

Real Firebase registration and Expo delivery need the app's project. Follow
[the phone checklist](../../packages/push/test/checklist.md#android-through-expo)
on the POCO X6 Pro in front, in the background and after `am kill`. Do not
use force-stop: Android stops delivery to a force-stopped app until it is
opened again. HyperOS battery and autostart settings can further restrict
delivery. Keep the urgency on the server so the app can reconcile on return.
