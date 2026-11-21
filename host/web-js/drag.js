// @ref LLP 1094.000 D2, D3 — a drag's data on the JS target, bundled only for a plan with `draggable`
// (`drag.flag`, LLP 1047.002). The page has no Rust runner, so it ports the runner's admission
// (`runner/src/runner/drag_files.rs`) with the browser's reach: a shipped `assets/` file is kept, an `app:/` file is
// refused, since its bytes cannot be read inside `dragstart`, and so is a `url` that is not absolute `http:` or
// `https:`. A prop left with nothing is removed. As the runner journals a refusal when a binding writes a new value,
// the page journals one when the value changes. `install` takes the runtime's hooks and journal, as follow.js
// does, so the rules load without a page. The drag itself is the browser's, its data `drag-out.js`'s.
import { listen } from '../web/drag-out.js';

const FILES = 'data-datatransferfiles', URL_PROP = 'data-datatransferurl';

/** Whether `entry` is a shipped file every host opens (`exact_plan::is_shipped_drag_file`). */
export function shipped(entry) {
  if (!entry.startsWith('assets/')) return false;
  const rest = entry.slice('assets/'.length);
  return rest.length > 0 && rest.length <= 128 && /^[\x20-\x7e]+$/.test(rest) && !/[:?#%\\]/.test(rest)
    && !rest.startsWith('/') && rest.split('/').every((s) => s && s !== '.' && s !== '..');
}

/** Whether `url` is absolute `http:` or `https:` with a host (`share.rs`'s `absolute_http`). */
export function absolute(url) {
  const scheme = /^https?:\/\//i.exec(url);
  const rest = scheme ? url.slice(scheme[0].length) : '';
  return rest !== '' && !/^[/?#]/.test(rest) && !/[\p{White_Space}\p{Cc}]/u.test(url);
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
  if (!url || absolute(url)) return url ?? '';
  refuse(`${url} is not an absolute http: or https: URL`);
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
  if (document) listen(document);
}
