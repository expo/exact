import { expect, test } from 'bun:test';
import { implementation } from './index.js';
import { videoFrameRate } from './metadata.js';

test('a camera stream records in the app and stops at maxDuration on the session clock', async () => {
  let now = 0;
  const timers = [], writes = [], released = [];
  const tracks = {
    lookup: id => id === 'stream' ? { kind: 'stream', tracks: ['camera'] } : { kind: 'video' },
    subscribe(id, consumer) { consumer.live(null); return () => released.push(id); },
  };
  const impl = implementation({ agent: true, now: () => now, changed() {}, tracks,
    files: { write: async (path, bytes) => writes.push({ path, bytes }) },
    after(ms, fire) { const timer = { at: now + ms, fire }; timers.push(timer); return () => timers.splice(timers.indexOf(timer), 1); },
  });
  const start = await impl.startMediaRecorder({ stream: 'stream', path: 'app:/data/clip', maxDuration: 5000 });
  expect(start.state).toBe('recording');
  now = 2000;
  await impl.pauseMediaRecorder({ recorder: start.id });
  now = 12000;
  await impl.resumeMediaRecorder({ recorder: start.id });
  expect(timers[0].at).toBe(15000);
  now = 15000;
  timers[0].fire();
  const result = await impl.stopMediaRecorder({ recorder: start.id });
  expect(result).toMatchObject({ path: 'app:/data/clip.mp4', mimeType: 'video/mp4', duration: 300, width: 16, height: 16, frameRate: 10 });
  expect(result.size).toBe(writes[0].bytes.byteLength);
  expect(videoFrameRate(writes[0].bytes, result.mimeType)).toBe(result.frameRate);
  expect(new TextDecoder().decode(writes[0].bytes.slice(4, 8))).toBe('ftyp');
  expect(released).toEqual(['camera']);
  expect(await impl.mediaRecorderState({ recorder: start.id })).toMatchObject({ state: 'inactive', reason: 'max-duration', result });
  expect(await impl.stopMediaRecorder({ recorder: start.id })).toEqual(result);
});

test('WebM rate follows video timestamps across clusters, independent of audio and DefaultDuration', () => {
  const bytes = (...parts) => new Uint8Array(parts.flatMap(p => [...p]));
  const element = (id, value, unknown = false) => bytes(id, [unknown ? 255 : 128 + value.length], value);
  const video = element([0xae], bytes(
    element([0xd7], [2]), element([0x83], [1]),
    element([0x23, 0xe3, 0x83], [0x01, 0xfc, 0xa0, 0x55]), // 33,333,333 ns, the initial 30 fps.
  ));
  const audio = element([0xae], bytes(element([0xd7], [1]), element([0x83], [2])));
  const tracks = element([0x16, 0x54, 0xae, 0x6b], bytes(audio, video));
  const block = (track, time) => element([0xa3], [128 + track, time >> 8, time & 255, 128, 0]);
  const cluster = (time, blocks, unknown) => element([0x1f, 0x43, 0xb6, 0x75], bytes(
    element([0xe7], [time >> 8, time & 255]), ...blocks,
  ), unknown);
  for (const unknown of [false, true]) {
    const file = element([0x18, 0x53, 0x80, 0x67], bytes(tracks,
      cluster(0, [block(2, 0), block(1, 20), block(2, 200)], unknown),
      cluster(400, [block(2, 0), block(1, 50), block(2, 200)], unknown),
    ), true);
    expect(videoFrameRate(file.buffer, 'video/webm')).toBe(5);
  }
});
