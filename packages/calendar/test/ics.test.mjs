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
