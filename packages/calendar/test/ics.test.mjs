// The web's `.ics` file (RFC 5545): no value an app passes breaks a content line, so none adds a property or an
// event of its own.
import { expect, test } from 'bun:test';
import { ics } from '../web/index.js';

const injected = '\r\nSUMMARY:injected\r\nEND:VEVENT\r\nBEGIN:VEVENT\nSUMMARY:second\rLOCATION:here';

test('a line break in any value stays inside its content line', () => {
  const file = ics({
    title: `Title${injected}`, start: '2026-11-13T09:00:00', uid: `uid${injected}`, duration: `PT1H${injected}`,
    url: `https://example.com/${injected}`, description: `Notes${injected}`, location: `Place${injected}`,
  }, 0);
  // Unfolded (§3.1), each content line is one property of the event.
  const lines = file.replace(/\r\n /g, '').split('\r\n').filter(Boolean);
  expect(lines.map(l => /^[A-Z-]+/.exec(l)?.[0])).toEqual(['BEGIN', 'VERSION', 'PRODID', 'BEGIN', 'UID', 'DTSTAMP', 'DTSTART', 'DURATION',
    'SUMMARY', 'DESCRIPTION', 'LOCATION', 'URL', 'END', 'END']);
  expect(lines.every(l => !/[\r\n]/.test(l))).toBe(true);
  expect(lines.find(l => l.startsWith('UID:'))).toBe('UID:uid\\nSUMMARY:injected\\nEND:VEVENT\\nBEGIN:VEVENT\\nSUMMARY:second\\nLOCATION:here');
  expect(lines.find(l => l.startsWith('URL:'))).toBe('URL:https://example.com/SUMMARY:injectedEND:VEVENTBEGIN:VEVENTSUMMARY:secondLOCATION:here');
});

// A zoned start becomes the UTC instant its wall time names. In a gap the wall time is read with the offset before the
// transition, and in a fold it is the first occurrence (§3.3.5), whichever side of UTC the zone is on.
test.each([
  ['Europe/Berlin', '2026-03-29T02:30:00', '20260329T013000Z'], // gap: 02:30 CET is 03:30 CEST
  ['Europe/Berlin', '2026-10-25T02:30:00', '20261025T003000Z'], // fold: the first 02:30, CEST
  ['Australia/Sydney', '2026-04-05T02:30:00', '20260404T153000Z'], // fold: the first 02:30, AEDT
  ['Australia/Sydney', '2026-10-04T02:30:00', '20261003T163000Z'], // gap: 02:30 AEST is 03:30 AEDT
  ['America/New_York', '2026-03-08T02:30:00', '20260308T073000Z'], // gap: 02:30 EST is 03:30 EDT
  ['America/New_York', '2026-11-01T01:30:00', '20261101T053000Z'], // fold: the first 01:30, EDT
  ['Europe/Berlin', '2026-07-01T12:00:00', '20260701T100000Z'],
  ['Europe/Berlin', '2026-01-15T12:00:00', '20260115T110000Z'],
  ['Asia/Kolkata', '2026-06-01T09:00:00', '20260601T033000Z'],
  ['Australia/Lord_Howe', '2026-04-05T01:45:00', '20260404T144500Z'], // fold of 30 minutes: the first 01:45, +11
  ['Australia/Lord_Howe', '2026-10-04T02:15:00', '20261003T154500Z'], // gap of 30 minutes: 02:15 at +10:30
])('%s %s starts at %s', (timeZone, start, expected) => {
  const file = ics({ title: 'Meeting', start, timeZone, uid: 'u' }, 0);
  expect(file).toContain(`\r\nDTSTART:${expected}\r\n`);
});
