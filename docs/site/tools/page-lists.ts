export type ListedPage = { path: string; title: string; source: string; section: string };

// The pages under a `##` heading for each section, in their order. As in the sidebar, a guide split into pages
// lists them one level below itself: they share its source and follow it.
export function bySection<P extends ListedPage>(pages: P[], url: (p: P) => string, fields: (p: P) => string): string {
  const split = (p: P) => pages.findIndex(q => q.source === p.source) < pages.indexOf(p);
  return [...new Set(pages.map(p => p.section))]
    .map(section => [`## ${section}`, '', ...pages.filter(p => p.section === section).map(p => `${split(p) ? '    ' : ''}- [${p.title}](${url(p)})${fields(p)}`)].join('\n'))
    .join('\n\n');
}
