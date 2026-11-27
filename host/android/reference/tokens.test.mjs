import { expect, test } from 'bun:test';
import { metrics } from '../../../kernel/tables/schema.json';
import { rows } from './rows.json';
import { check, materialWeight, parseObjects } from './tokens.mjs';

test('only a whole font weight constant supplies a Material weight', () => {
  expect(materialWeight('FontWeight.Normal')).toBe(400);
  expect(materialWeight('FontWeight.Medium')).toBe(500);
  expect(materialWeight('FontWeight.Bold')).toBe(700);
  expect(materialWeight('FontWeight(FontWeight.Medium.weight + 100)')).toBeUndefined();
});

test.each([
  ['grouped-row-inset', 'ListItemStartPadding'],
  ['grouped-row-padding-end', 'ListItemEndPadding'],
])('%s has a Material cell traced to %s', (metric, name) => {
  expect(metrics.find(([key]) => key === metric)?.[6]).toBe(16);
  expect(rows[metric]).toEqual({ source: 'ListItem.kt', name });
});

test('Material token declarations discard line comments before resolving references', () => {
  const objects = parseObjects(`internal object TypeScaleTokens {
    val TitleSmallWeight = TypefaceTokens.WeightMedium // TODO: update this weight
    // A comment before the next declaration belongs to neither value.
    val TitleMediumWeight =
        TypefaceTokens.WeightMedium // Repeated in the generated source.
    val TitleLargeWeight = FontWeight.Normal
}
`);
  expect(objects.TypeScaleTokens).toEqual({
    TitleSmallWeight: 'TypefaceTokens.WeightMedium',
    TitleMediumWeight: 'TypefaceTokens.WeightMedium',
    TitleLargeWeight: 'FontWeight.Normal',
  });
});

test.each([
  ['16.sp', '{"sp":16}'],
  ['TypefaceTokens.MissingWeight', '{"ref":"TypefaceTokens.MissingWeight"}'],
  ['450', '450'],
  ['FontWeight.W900', 'FontWeight.W900'],
])('the Material audit reports the unrecognized weight %s', (expression, source) => {
  const objects = {
    TypeScaleTokens: { BodyLargeWeight: expression },
  };
  const logs = [];
  const log = console.log;
  console.log = (line) => { if (line.includes('body Weight:')) logs.push(line); };
  try {
    expect(check({ files: {}, objects })).toBe(false);
  } finally {
    console.log = log;
  }
  expect(logs).toEqual([`  DIFFERS body Weight: schema 400, source unrecognized ${source}   TypeScaleTokens.BodyLargeWeight`]);
});
