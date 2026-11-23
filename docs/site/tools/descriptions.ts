import type { Token, Tokens } from 'marked';
import type { NavPage } from '../constants/navigation.ts';

const DESCRIPTION_MIN = 40;
const DESCRIPTION_MAX = 180;
const SENTENCE_BREAK = /(?<=[.!?])(?<!\b(?:[a-z]\.){2,})(?<!\b(?:vs|cf)\.)\s+/gi;

export function decode(s: string): string {
  return s.replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, '&');
}

export function plain(tokens: Token[] | undefined, code = (text: string) => text): string {
  return (tokens ?? []).map(t => {
    if (t.type === 'codespan') return code(decode(t.text));
    if ('tokens' in t && t.tokens) return plain(t.tokens, code);
    return 'text' in t ? decode(String(t.text)) : '';
  }).join('');
}

function sentencesOf(tokens: Token[], proseOnly = false): string[] {
  const spans: string[] = [];
  const text = plain(tokens, code => `\u0000${spans.push(code) - 1}\u0000`);
  return text.trim().split(SENTENCE_BREAK)
    .filter(sentence => !proseOnly || /\p{L}/u.test(sentence))
    .map(sentence => sentence.replace(/\u0000(\d+)\u0000/g, (_, i: string) => spans[Number(i)]).replace(/\s+/g, ' ').trim());
}

function cutAtWord(text: string): string {
  const boundary = text.lastIndexOf(' ', DESCRIPTION_MAX - 3);
  const end = boundary >= DESCRIPTION_MIN - 1 ? boundary : DESCRIPTION_MAX - 1;
  // A hard cut can fall between the two UTF-16 units of a character.
  const prefix = text.slice(0, end).replace(/[\uD800-\uDBFF]$/, '');
  return `${prefix.trimEnd()}…`;
}

function listDescription(tokens: Token[]): string | undefined {
  for (const list of tokens) {
    if (list.type !== 'list') continue;
    let description = '';
    for (const { tokens: [first] } of (list as Tokens.List).items) {
      if (first?.type !== 'text' && first?.type !== 'paragraph') continue;
      const sentence = sentencesOf([first], true)[0];
      if (!sentence || sentence.endsWith(':')) continue;
      const combined = description ? `${description} ${sentence}` : sentence;
      if (combined.length > DESCRIPTION_MAX && description.length >= DESCRIPTION_MIN) break;
      description = combined.length <= DESCRIPTION_MAX ? combined : sentence;
      if (description.length > DESCRIPTION_MAX) return cutAtWord(description);
    }
    if (description.length >= DESCRIPTION_MIN) return description;
  }
}

export function descriptionOf(tokens: Token[], fallback: string): string {
  const description = tokens
    .filter((t): t is Tokens.Paragraph => t.type === 'paragraph')
    .flatMap(p => sentencesOf(p.tokens))
    .find(x => x.length >= DESCRIPTION_MIN && !x.endsWith(':')) ?? listDescription(tokens);
  if (description !== undefined) return description.length <= DESCRIPTION_MAX ? description : cutAtWord(description);
  if (fallback.length <= DESCRIPTION_MAX) return fallback;
  const boundary = [...fallback.matchAll(SENTENCE_BREAK)]
    .findLast(({ index }) => index >= DESCRIPTION_MIN && index <= DESCRIPTION_MAX)?.index;
  return boundary === undefined ? cutAtWord(fallback) : fallback.slice(0, boundary);
}

export function sectionDescriptionProblems(docs: { nav: Pick<NavPage, 'source' | 'split' | 'sectionDescriptions'>; sections: { anchor: string }[] }[]): string[] {
  return docs.flatMap(({ nav, sections }) => Object.entries(nav.sectionDescriptions ?? {}).flatMap(([anchor, description]) => {
    const entry = `${nav.source}: sectionDescriptions[${JSON.stringify(anchor)}]`;
    const problems: string[] = [];
    if (!nav.split || !sections.some(section => section.anchor === anchor)) problems.push(`${entry} does not name a section of a split guide`);
    if (!description.trim()) problems.push(`${entry} is empty`);
    return problems;
  }));
}
