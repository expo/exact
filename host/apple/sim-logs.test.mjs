import { test, expect } from 'bun:test';
import { escapePredicateString, logPredicate, parseLaunchedPid } from './sim-logs.mjs';

test('parseLaunchedPid reads PID from simctl launch output', () => {
  expect(parseLaunchedPid('com.exact.caltrain: 54321\n')).toBe(54321);
  expect(parseLaunchedPid('info line\ncom.example.app: 99\n')).toBe(99);
  expect(parseLaunchedPid('no pid here')).toBeNull();
});

test('logPredicate prefers PID filtering and escapes executable names with dev.exact fallback', () => {
  expect(logPredicate({ pid: 1234, executable: "Charlie's \"App\"" })).toBe('processID == 1234');
  expect(escapePredicateString('A "B" \\ C')).toBe('A \\"B\\" \\\\ C');
  expect(logPredicate({ pid: null, executable: 'My "App"' })).toBe(
    'process == "My \\"App\\"" OR subsystem == "dev.exact" OR subsystem BEGINSWITH "dev.exact."',
  );
});
