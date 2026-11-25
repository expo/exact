import { expect, test } from 'bun:test';
import { marked } from 'marked';
import { descriptionOf, sectionDescriptionProblems } from '../tools/descriptions.ts';
import { markdownHeadings, pageMarkdown } from '../tools/markdown-headings.ts';

const fallback = 'The guide describes the behavior of apps on each platform.';
const derive = (markdown: string) => descriptionOf(marked.lexer(markdown), fallback);

test('split-page Markdown keeps a first indented code block', () => {
  const tokens = marked.lexer('## A section\n\n    first()\n    second()\n\nThe next paragraph.\n');
  const markdown = pageMarkdown(tokens, raw => raw, 'A section');
  expect(markdown).toBe('# A section\n\n    first()\n    second()\n\nThe next paragraph.');
});

test('Markdown section boundaries include setext headings and skip fenced code', () => {
  const markdown = [
    'Recipe', '======', '', '   ````ts', '## Inside', '```', '## After a short marker',
    '~~~~', '## After a different marker', '```` trailing text', '## After trailing text',
    '  `````', '', '   ## C#', '', 'First section.', '', 'Setext heading', '--------------', '',
    'Second section.', '', '## Closing hashes ###', '',
  ].join('\n');
  expect(markdownHeadings(markdown).map(({ level, text, start, headingEnd, end }) => ({
    level, text, heading: markdown.slice(start, headingEnd).trimEnd(), section: markdown.slice(start, end).trimEnd(),
  }))).toEqual([
    { level: 1, text: 'Recipe', heading: 'Recipe\n======', section: markdown.trimEnd() },
    { level: 2, text: 'C#', heading: '   ## C#', section: '   ## C#\n\nFirst section.' },
    { level: 2, text: 'Setext heading', heading: 'Setext heading\n--------------', section: 'Setext heading\n--------------\n\nSecond section.' },
    { level: 2, text: 'Closing hashes', heading: '## Closing hashes ###', section: '## Closing hashes ###' },
  ]);
});

test('Sound lists its opening sentences when the next sentence starts with code', () => {
  const sound = [
    '- **A local speech recognizer may still need a language download.**',
    '  `speechRecognitionStatus.local` describes the engine, not its installed languages.',
    '- **An audio player is silent in iPhone Silent mode.** The default audio session is `ambient`.',
    '- **A scheduled sound plays after Stop.** A sequencer can schedule each step ahead.',
    '- **The first sound on the web is silent.** A sound a page plays on load is dropped.',
  ].join('\n');
  expect(derive(sound)).toBe('A local speech recognizer may still need a language download. An audio player is silent in iPhone Silent mode. A scheduled sound plays after Stop.');
});

test('a long sentence is cut at a word when its only earlier period ends e.g.', () => {
  const start = 'Load a configuration document from a local source, e.g. ';
  const sentence = `${start}${'a folder with data '.repeat(13).trim()}.`;
  const result = derive(sentence);
  expect(result.startsWith(`${start}a folder`)).toBe(true);
  expect(result.endsWith('…')).toBe(true);
  expect(result.length).toBeLessThanOrEqual(180);
});

test('vs. stays inside a sentence before a separate lowercase API sentence', () => {
  const sentence = 'This guide compares the memory cost of native vs. web rendering.';
  expect(derive(`${sentence} renderOnWeb builds the preview.`)).toBe(sentence);
});

test('a full stop at character 180 fits before a lowercase API name', () => {
  const sentence = `The setting accepts ${'word, '.repeat(24)}or a local path.`;
  expect(sentence).toHaveLength(180);
  expect(derive(`${sentence} pathOf returns the resolved path.`)).toBe(sentence);
});

test('a colon-ending item leaves a complete list description', () => {
  const first = 'The application keeps its settings in a local file.';
  expect(derive(`- ${first}\n- This file contains these fields:`)).toBe(first);
});

test('a list shorter than 40 characters uses the fallback', () => {
  expect(derive('- Ports are open.')).toBe(fallback);
});

test('a list uses the next item when the opening is short and both cannot fit', () => {
  const second = `The listener reports occupancy and ${'availability, '.repeat(7)}then closes.`;
  expect(derive(`- The port is open on the local device.\n- ${second}`)).toBe(second);
});

const code = 'signalPorts.dispatch("exact.running", true);';
test('a list item opening with a code block uses the fallback', () => {
  expect(derive(`- \`\`\`js\n  ${code}\n  \`\`\``)).toBe(fallback);
});

test('a list item containing only inline code uses the fallback', () => {
  expect(derive(`- \`${code}\``)).toBe(fallback);
});

test('a list item containing only code spans and punctuation uses the fallback', () => {
  expect(derive('- `windowManagementStatus()`: `{available, state}`.')).toBe(fallback);
});

test('an overlong fallback ends at the last complete sentence that fits', () => {
  const table = '| Setting | Value |\n| --- | --- |\n| Location | Local folder |';
  const complete = 'The guide describes how the app opens a document from a local folder. It also covers where each window opens.';
  const inherited = `${complete} The same configuration can set the initial location for each open window and choose whether to restore the previous session.`;
  expect(descriptionOf(marked.lexer(table), inherited)).toBe(complete);
});

test('a cut without spaces keeps whole Unicode code points', () => {
  const prefix = '界'.repeat(177);
  expect(derive(`${prefix}界😀.`)).toBe(`${prefix}界…`);
  expect(derive(`${prefix}😀。続`)).toBe(`${prefix}😀…`);
});

test('section overrides report every empty value and unmatched key together', () => {
  const written = 'A written description for a section published by the docs site.';
  expect(sectionDescriptionProblems([
    {
      nav: {
        source: 'guide.md',
        split: true,
        sectionDescriptions: { '': written, mistyped: written, empty: '', blank: ' \n', valid: written },
      },
      sections: [{ anchor: 'empty' }, { anchor: 'blank' }, { anchor: 'valid' }],
    },
    {
      nav: { source: 'reference.md', split: true, sectionDescriptions: { renamed: written } },
      sections: [],
    },
    {
      nav: { source: 'pitfalls.md', sectionDescriptions: { unsplit: written } },
      sections: [],
    },
  ])).toEqual([
    'guide.md: sectionDescriptions[""] does not name a section of a split guide',
    'guide.md: sectionDescriptions["mistyped"] does not name a section of a split guide',
    'guide.md: sectionDescriptions["empty"] is empty',
    'guide.md: sectionDescriptions["blank"] is empty',
    'reference.md: sectionDescriptions["renamed"] does not name a section of a split guide',
    'pitfalls.md: sectionDescriptions["unsplit"] does not name a section of a split guide',
  ]);
});
