import { test, expect, mock } from 'bun:test';

// A Rust page that does not wait for its module asks for MIDI before the module's grants are known; the host's port
// report (midi-glue.js: `unavailable` at install on a browser with no Web MIDI) can land first. It is kept, and
// applied when the grant arrives, as a native host that knows the grant at boot applies it.
mock.module('./navigation.js', () => import('../web/navigation.js'));
mock.module('./prompt-glue.js', () => ({}));
mock.module('./alert-message.js', () => ({}));
mock.module('./menu-rows.js', () => ({}));
mock.module('./names.js', () => ({ sourceTypes: {} }));
const { data } = await import('./rt.js');
const { midi } = await import('./midi.js');

test('a MIDI port report that arrives before the grant applies once it does', () => {
  midi({}, true);
  expect(exact.midi.state().state).toBe('denied');
  exact.midi.deliver({ ports: { state: 'unavailable' } });
  expect(exact.midi.state().state).toBe('denied');
  data.grants = 'device.midi\n';
  for (const f of data.q.splice(0)) f();
  expect(exact.midi.state().state).toBe('unavailable');
});
