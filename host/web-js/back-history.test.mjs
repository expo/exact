import { expect, test } from 'bun:test';
import { renderedPages } from './rendered-pages.mjs';

test('document titles follow the active route and fall back to its heading with head title=none', async () => {
  await renderedPages(`routes nav
  tab home "/" render=build
    detail "/detail" render=build
    plain "/plain" render=build

component Heads
  view
    main navigationKey=\`\${top(nav).id}\` navigationBack="back"
      head description="Root description"
      for e in stack(nav) key=e.id
        column navigationKey=\`\${e.id}\`
          if e.name == "plain"
            header
              text "Plain heading" role="heading"
            head title=none
          if e.name == "home"
            column
              head title="Home" description="Home description"
          if e.name == "detail"
            head title="Detail" description="Detail description"
          text e.name
`, page => {
    expect(page('')).toEqual(['Home', 'Home description']);
    expect(page('detail')).toEqual(['Detail', 'Detail description']);
    expect(page('plain')).toEqual(['Plain heading', 'Root description']);
  });
}, 30000);
