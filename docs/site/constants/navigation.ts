import { readdirSync } from 'node:fs';
import { join } from 'node:path';

export type NavPage = {
  route: string;
  path: string;
  source: string;
  description?: string;
  split?: boolean;
  sectionDescriptions?: Record<string, string>;
  hidden?: boolean;
  landing?: boolean;
};

export type NavTab = 'home' | 'learn' | 'reference';

export type NavSection = {
  title: string;
  tab: NavTab;
  card?: { title: string; description: string };
  pages: NavPage[];
};

const DOCS = join(import.meta.dir, '../..');

function pagesFromFolder(folder: string, routePrefix: string, descriptionOverrides: Record<string, string> = {}): NavPage[] {
  const sources = new Map<string, string>();
  return readdirSync(join(DOCS, folder), { withFileTypes: true })
    .filter(file => file.isFile() && file.name.endsWith('.md'))
    .map(file => file.name.slice(0, -3))
    .sort()
    .map(name => {
      const words = name.split('-').filter(Boolean).map(word => word[0].toUpperCase() + word.slice(1));
      const route = routePrefix + words.join('');
      const source = `${folder}/${name}.md`;
      const previous = sources.get(route);
      if (previous) throw new Error(`${previous} and ${source} both use route ${route}`);
      sources.set(route, source);
      const description = Object.hasOwn(descriptionOverrides, name) ? descriptionOverrides[name] : undefined;
      return { route, path: `/${folder}/${name}`, source, description };
    });
}

export const sections: NavSection[] = [
  {
    title: 'Home',
    tab: 'home',
    pages: [
      {
        route: 'home',
        path: '/',
        source: 'site/pages/home.md',
        landing: true,
        description: 'Write an app once. It runs natively on the web, iOS, macOS, Android, Linux, and more.',
      },
    ],
  },
  {
    title: 'Get started',
    tab: 'home',
    card: { title: 'Start here', description: 'Build an app with Exact: the idea, the workflow, and the one screen every app starts from.' },
    pages: [
      {
        route: 'startHere',
        path: '/start',
        source: 'start-here.md',
        description: 'Build an app with Exact: the idea, the workflow, and the one screen every app starts from.',
      },
      {
        route: 'startHereSite',
        path: '/start-site',
        source: 'start-here-site.md',
      },
    ],
  },
  {
    title: 'Learn',
    tab: 'learn',
    card: { title: 'Learn', description: 'Contract, the language of an Exact app, from the first component to delivery.' },
    pages: [
      {
        route: 'contractForHumans',
        path: '/learn/contract-for-humans',
        source: 'contract-for-humans.md',
        split: true,
        description: 'Contract, the language of an Exact app, from the first component to delivery.',
      },
      {
        route: 'contractForAgents',
        path: '/learn/contract-for-agents',
        source: 'contract-for-agents.md',
        split: true,
        description: 'Contract for an agent building an app: lookup by section for what no recipe covers.',
        sectionDescriptions: {
          'values-expressions-and-functions': 'An expression Contract accepts means what the same TypeScript means, except as the section\'s table says, and a TypeScript form Contract refuses is refused with what to write.',
          'repair-common-mistakes': 'The forms the compiler refuses or that mislead, each with what to write instead, and the rules for repairing a refusal.',
        },
      },
    ],
  },
  {
    title: 'Recipes',
    tab: 'home',
    card: { title: 'Recipes', description: 'Short answers to common questions, one page each: forms, lists, timers, sign-in, storage, and more.' },
    pages: pagesFromFolder('recipes', 'recipe', {
      'alternate-app-icons': 'Icons the person chooses for the Home Screen: alternate_icons in app.json, and @exact/app-icon reads and asks for the choice on iOS.',
      'app-json': 'app.json names the app and what it ships; its $schema gives an editor every key.',
      'background-transfers': '@exact/background-transfer hands an upload or download to the system, which finishes it after the app is suspended or terminated.',
      'chat-thread': 'A transcript open at its newest message, over Messages\' composer.',
      combobox: 'Filter options while a field keeps focus and names its active option.',
      'content-design': 'Chrome is the platform\'s; the content is yours: rows, grouping, where actions sit.',
      convex: '@exact/convex keeps Convex queries live over one socket, with nothing polled.',
      displays: '@exact/window-management (LLP 1178): the displays by the web\'s Window Management API, and where the pointer is, which the web cannot ask.',
      'drag-out': 'A node a person will lift and drop into another app says so with HTML\'s draggable attribute and declares what the drag carries.',
      'edit-sheet': 'A row that opens its item\'s sheet is a link: a tap, a deep link and Back all arrive at follow.',
      'editable-rows': 'Rows added and removed while editing: each row\'s remove is a red minus-circle and a swipe; Add is the section\'s last row.',
      'fetch-loading-error': 'A fetched resource with loading, an error and Retry, whose rows belong to the owner that asked for them.',
      'files-and-grants': 'Where an app keeps what it keeps, and the line of app.ts\'s grants that reaches it.',
      foldables: 'A foldable (the iPhone Duo) splits its inner display at the hinge when partially folded.',
      'gpu-module': 'A canvas that the app\'s GPU module draws in Rust: a gpu/ crate beside app.json.',
      'gpu-module-android': 'On Android, a GPU surface whose crates call into Java needs the app\'s Java VM and Context before its first request.',
      'in-app-browser': '@exact/in-app-browser opens a web page over the app, such as a checkout or a help page.',
      'ios-driving': 'Check a screen against a reference with compare, not by eye: bun exact.mjs agent ios captures as a device does and says where they differ.',
      'keyboard-and-focus': 'Keep typing or dismiss the keyboard: the Return key, focus() and blur() from an action, and what a scroll does.',
      'local-network': 'A light, a printer or a NAS is found by DNS-SD with @exact/local-network and reached with fetch (LLP 1133.036).',
      'long-lists': 'A long list or grid that loads more: virtualized rows, each holding N cards.',
      'mobile-build': 'Release builds for a phone: the commands, signing, and where the artifact lands.',
      money: 'Keep money as a whole number of cents, and format it at the edge with formatNumber(dollars, "currency", "USD").',
      motion: 'Motion is CSS\'s: transition, keyframes and animation, -exact-exit-animation and -exact-layout-transition.',
      'native-module': 'Sound, MIDI and a transport are built in (guide, apps/metronome): write no module.',
      'native-views': 'A view needing a platform API (a map, the camera) is a Swift view: a hyphenated tag in app.json modules, made by modules/apple/.swift.',
      'navigation-stack': 'A list pushing a detail, a Cancel/Save sheet, an alert (pattern list-detail).',
      'number-field': 'inputmode="decimal" is the iPhone\'s number pad with a decimal point, "numeric" digits only; without one, the full keyboard.',
      oauth: '@exact/oauth signs a person in and keeps them signed in: PKCE in the system browser, refresh, requests with the token, sign-out.',
      permissions: 'A native module that needs the person\'s permission (location here) is asked for in the app\'s own UI, read as a fact, and recovered from in the app\'s own UI.',
      'phone-manifest': 'A phone app\'s manifest: orientation, appearance, HTTP, and a public setting.',
      'porting-and-release': 'Name the port when you make it (exact new <path> --name and --bundle-id), then a release build and splitting the work.',
      'ports-and-browsers': 'The port that web, test web and agent web serve on, the Bun and Chrome the launchers use, and the stores a drive keeps or resets.',
      properties: 'In app.test.contract, state what should hold across the app\'s interactions.',
      'random-and-shuffle': 'The data module refuses Math.random(): deal from exactTime()\'s seed, with a stream for each thing dealt.',
      'responsive-columns': 'Columns side by side when wide and stacked on a phone: flex items that wrap at a minimum width, as in CSS.',
      'rest-backend': 'The REST starter creates a shared-list app and a Bun server.',
      'shipped-data': 'Data the app ships and only reads is a JSON file app.ts imports: it is bundled with the data module on every host, with no grant and no path to get wrong.',
      'snapback-auth': 'Use the complete mixed identity app for the schema, Contract, TypeScript and browser tests.',
      'snapback-tests': 'A Snapback app needs its backend running before test web.',
      snapback: 'For shared server data, exact new app --snapback writes a guestbook and wires its driver, generated operations and device.',
      'socket-ticket': 'A WebSocket whose server checks the signed-in person at connect opens with a single-use ticket bought over HTTPS with @exact/oauth.',
      'store-update': '@exact/store-update says whether the App Store or Google Play has a newer binary and opens it, for a change Exact\'s own signed updates cannot carry.',
      'suspending-feed': 'A WebSocket feed that stays open while a desktop window is hidden and closes when the host freezes the page.',
      symbols: 'image "symbol:<role>" is an icon each host draws from its own set (SF Symbols on Apple, matching outlines elsewhere).',
      tabs: 'Use tabs for at least two top-level sections, each with its own route stack.',
      tests: 'app.test.contract drives the app by testId, the same on the web and iOS (bun exact.mjs test web, test ios; verify runs them too).',
      'timer-in-a-component': 'A bounded child owns its timer; an activity gate stops it while hidden.',
      timers: 'A task is the only timer: every(ms, action) repeats, after(ms, action) fires once, and when cond arms it while it holds.',
      website: 'A website in Exact: when a page is made, how to serve it, and a setting.',
      'white-label': 'A brand with its own store app is its own app, not a variant: a directory per brand, in a Bun workspace whose packages hold the screens and the data code.',
    }),
  },
  {
    title: 'Site',
    tab: 'home',
    pages: [
      {
        route: 'components',
        path: '/_components',
        source: 'site/fixtures/components.md',
        hidden: true,
        description: 'Every component the site renders, for its tests.',
      },
    ],
  },
  {
    title: 'Reference',
    tab: 'reference',
    pages: [
      {
        route: 'contractGrammar',
        path: '/reference/contract-grammar',
        source: 'contract-grammar.md',
        split: true,
        description: 'The Contract grammar and vocabulary the compiler accepts: every declaration, element and attribute.',
        sectionDescriptions: {
          declarations: 'The grammar of a Contract file\'s declarations, such as shape, fn, source, style, keyframes, routes and component, and the limits the compiler checks on them.',
          'host-commands': 'The host commands an action runs as statements, such as focus, navigate, share and haptic, and a working implementation to start from.',
        },
      },
      {
        route: 'reference',
        path: '/reference/developer',
        source: 'reference.md',
        split: true,
        description: 'Toolchains, the Contract CLI, the native and web loops, data modules, delivery and the stress examples.',
        sectionDescriptions: {
          'run-a-terminal-app': 'Build the terminal harness and the LLP reader, and the command line they share with exact-terminal: a launch location, a single run with --print or --json, and headless drives.',
          'access-hatches': 'An access hatch hands the app\'s native code the platform object Exact built for a node, at defined moments, so the app can do what only that object can.',
          'the-six-checks': 'The six checks of a change: build, test, lint, caps, the boot graph and the workspace, with what each covers and what the async lane runs.',
          'rust-live-replacement': 'Rust replacement is enabled by default in development and production; app.json\'s rust key disables it or sets a mode for an environment or a platform.',
        },
      },
      {
        route: 'agentPitfalls',
        path: '/reference/pitfalls',
        source: 'agent-pitfalls.md',
        split: true,
        description: 'Verified footguns in writing Exact apps, each as symptom, cause and what to do.',
      },
      {
        route: 'gamePitfalls',
        path: '/reference/game-pitfalls',
        source: 'game-pitfalls.md',
        description: 'Verified footguns in writing Exact games, each as symptom, cause and what to do.',
      },
    ],
  },
  {
    title: 'For AI agents',
    tab: 'home',
    pages: [
      {
        route: 'agents',
        path: '/agents',
        source: 'site/pages/agents.md',
        description: 'Give your coding agent these docs, or let it read any page as Markdown.',
      },
    ],
  },
];

export const tabs: { id: NavTab; title: string; icon: string }[] = [
  { id: 'home', title: 'Home', icon: 'symbol:home' },
  { id: 'learn', title: 'Learn', icon: 'symbol:book' },
  { id: 'reference', title: 'Reference', icon: 'symbol:document' },
];

// The home page's For AI agents rows, in its HTML and its Markdown. Each opens a section of pages/agents.md.
export const agentRows: { title: string; detail: string; anchor: string }[] = [
  { title: 'Start prompt', detail: 'The one file to read first', anchor: 'start-prompt' },
  { title: 'Markdown pages', detail: 'Add .md to any page URL', anchor: 'markdown-pages' },
  { title: 'llms.txt', detail: 'Every page in one index', anchor: 'llmstxt' },
  { title: 'sitemap.md', detail: 'Pages with type and prerequisites', anchor: 'sitemapmd' },
  { title: 'Open in your agent', detail: 'Claude, ChatGPT, Cursor, Codex', anchor: 'open-in-your-agent' },
  { title: 'Terminal lookups', detail: "Answers from an app's folder", anchor: 'terminal-lookups' },
  { title: 'Feedback', detail: 'Report an error in a page', anchor: 'feedback' },
];
