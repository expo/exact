// @ref LLP 1094.000 D2, D3 — a drag's data on the JS target, bundled only for a plan with `draggable`
// (`drag.flag`, LLP 1047.002). The page has no Rust runner, so it ports the runner's admission
// (`runner/src/runner/drag_files.rs`) with the browser's reach: a shipped `assets/` file is kept, an `app:/` file is
// refused, since its bytes cannot be read inside `dragstart`, and so is a `url` that is not absolute `http:` or
// `https:`. A prop left with nothing is removed. As the runner journals a refusal when a binding writes a new value,
// the page journals one when the value changes. `install` takes the runtime's hooks and journal, as follow.js
// does, so the rules load without a page. The drag itself is the browser's, its data `drag-out.js`'s.
import { listen, writeDraggable } from '../web/drag-out.js';
import { httpUrlRefusal } from './url.js';

const FILES = 'data-datatransferfiles', URL_PROP = 'data-datatransferurl';

/** Whether `entry` is a shipped file every host opens (`exact_plan::is_shipped_drag_file`). */
export function shipped(entry) {
  if (!entry.startsWith('assets/')) return false;
  const rest = entry.slice('assets/'.length);
  return rest.length > 0 && rest.length <= 128 && /^[\x20-\x7e]+$/.test(rest) && !/[:?#%\\]/.test(rest)
    && !rest.startsWith('/') && rest.split('/').every((s) => s && s !== '.' && s !== '..');
}

/** The entries the page carries, each refusal said in the runner's words. */
export function admit(files, refuse) {
  return (files ?? '').split('\n').filter(Boolean).filter((entry) => {
    if (shipped(entry)) return true;
    refuse(entry.startsWith('app:/') ? `${entry}: an app:/ file cannot be dragged on the web` : `${entry} is neither a shipped assets/ file nor an app:/ path`);
    return false;
  }).join('\n');
}

/** The `url` the page carries, or '' with the refusal said. */
export function admitUrl(url, refuse) {
  const why = url ? httpUrlRefusal(url) : null;
  if (!why) return url ?? '';
  // The URL itself stays out of the journal: it may hold a password or a token.
  refuse(`url is not an absolute http: or https: URL: ${why}`);
  return '';
}

export function install({ PropHooks, say, document = globalThis.document }) {
  const last = new WeakMap();
  const hook = (name, rule) => (PropHooks[name] = (e, v) => {
    const seen = last.get(e) ?? last.set(e, {}).get(e);
    if (seen[name] === v) return true;
    seen[name] = v;
    const kept = v == null ? '' : rule(v, (reason) => say(`drag: refused: ${reason}`));
    if (kept) e.setAttribute(name, kept);
    else e.removeAttribute(name);
    return true;
  });
  hook(FILES, admit);
  hook(URL_PROP, admitUrl);
  // A render on the server (main-server.js) admits the data and hears no drag.
  if (!document) return;
  // The listener holds a node's drag off while a field in it keeps a contact, so it writes `draggable` (drag-out.js).
  PropHooks.draggable = (e, v) => (writeDraggable(e, v), true);
  listen(document, (e) => e.$exactTree === true);
}
