import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';

test('the calibration lookup contains the native sources the fixture compiles', () => {
  const recipe = readFileSync(new URL('../../../../docs/reference.md', import.meta.url), 'utf8').split("### A camera track's calibration\n")[1].split('### Save an edit of a photo')[0];
  for (const [language, path] of [['kotlin', '../android/Calibration.kt'], ['swift', '../apple/CameraCalibration.swift']]) {
    const source = readFileSync(new URL(path, import.meta.url), 'utf8');
    expect(recipe.split('```' + language + '\n')[1]?.split('```')[0]).toBe(source);
  }
});
