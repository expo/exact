import { marked, type Token } from 'marked';

export function pageMarkdown(tokens: Token[], rewrite: (raw: string) => string, sectionTitle?: string) {
  const content = sectionTitle === undefined ? tokens : tokens.slice(1);
  const body = content.map(token => token.type === 'code' ? token.raw : rewrite(token.raw)).join('');
  return sectionTitle === undefined ? body.trim() : `# ${sectionTitle}\n\n${body.replace(/^(?:[ \t]*\n)+/, '').trimEnd()}`;
}

export function markdownHeadings(markdown: string) {
  let offset = 0;
  const headings = marked.lexer(markdown, { gfm: true }).flatMap(token => {
    const start = offset;
    offset += token.raw.length;
    return token.type === 'heading' ? [{ level: token.depth, text: token.text.trim(), start, headingEnd: offset }] : [];
  });
  return headings.map((heading, i) => ({
    ...heading,
    end: headings.slice(i + 1).find(next => next.level <= heading.level)?.start ?? markdown.length,
  }));
}
