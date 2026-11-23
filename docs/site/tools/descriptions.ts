import type { Token, Tokens } from 'marked';
import type { NavPage } from '../constants/navigation.ts';

const DESCRIPTION_MIN = 40;
const DESCRIPTION_MAX = 180;

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
  // Keep periods in code and dotted abbreviations such as e.g. within the sentence.
  return text.trim().split(/(?<=[.!?])(?<!\b(?:[A-Za-z]\.){2,})\s+/)
    .filter(sentence => !proseOnly || !/^(?:\u0000\d+\u0000\s*)+[.!?]?$/.test(sentence))
    .map(sentence => sentence.replace(/\u0000(\d+)\u0000/g, (_, i: string) => spans[Number(i)]).replace(/\s+/g, ' ').trim());
}

function cutAtWord(text: string): string {
  const boundary = text.lastIndexOf(' ', DESCRIPTION_MAX - 3);
  return `${text.slice(0, boundary >= DESCRIPTION_MIN - 1 ? boundary : DESCRIPTION_MAX - 1).trimEnd()}…`;
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
    .find(x => x.length >= DESCRIPTION_MIN && !x.endsWith(':')) ?? listDescription(tokens) ?? fallback;
  return description.length <= DESCRIPTION_MAX ? description : cutAtWord(description);
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
