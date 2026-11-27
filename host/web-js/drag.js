// @ref LLP 1094.000 D2, D3, D14 — a drag's data on the JS target, bundled only for a plan with `draggable`
// (`drag.flag`, LLP 1047.002). The page has no Rust runner, so it ports the runner's admission
// (`runner/src/runner/drag_files.rs`) with the browser's reach: a shipped `assets/` file is kept, an `app:/` file is
// refused, since its bytes cannot be read inside `dragstart`, and so is a `url` that is not absolute `http:` or
// `https:`. The files and names a plan binds as one value (`data-datatransferentries`) are decoded and admitted pair
// by pair into `data-datatransferfiles` and `data-datatransfernames`, by D14's name rule (`plan/src/drag_name.rs`).
// A prop left with nothing is removed. A refusal is journaled once while it stands, as the runner's is. `install`
// takes the runtime's hooks and journal, as follow.js does, so the rules load without a page. The drag itself is the
// browser's, its data `drag-out.js`'s.
import { listen, writeDraggable } from '../web/drag-out.js';
import { httpUrlRefusal } from './url.js';

const FILES = 'data-datatransferfiles', URL_PROP = 'data-datatransferurl';
const ENTRIES = 'data-datatransferentries', NAMES = 'data-datatransfernames';
/** Separates the names from the files in the entries (`exact_plan::DRAG_NAMES_MARK`). */
const NAMES_MARK = '\u001e';

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

/** Why `name` is no name every host can write, as written, or null (`exact_plan::drag_name_refusal`). */
export function nameRefusal(name) {
  if (!name || new TextEncoder().encode(name).length > 255) return 'a name is 1 to 255 bytes of UTF-8';
  if (/[\u0000-\u001f\u007f]/.test(name)) return 'a name holds no control character';
  if (/[<>:"/\\|?*]/.test(name)) return 'a name holds none of < > : " / \\ | ? *';
  if (name.startsWith(' ') || name.endsWith(' ')) return 'a name neither begins nor ends with a space';
  if (name.endsWith('.')) return 'a name does not end with a dot';
  if (/^(CON|PRN|AUX|NUL|(COM|LPT)[1-9¹²³])$/i.test(name.split('.')[0])) return 'Windows keeps that name for a device';
  return null;
}

/** The suffixes that spell one type in the extension table `expect drag` reads. */
const ALIASES = [['jpg', 'jpeg'], ['htm', 'html'], ['txt', 'text'], ['md', 'markdown']];
const suffix = (name) => { const dot = name.lastIndexOf('.'); return dot > 0 && dot < name.length - 1 ? name.slice(dot + 1) : null; };
const sameType = (a, b) => a.toLowerCase() === b.toLowerCase()
  || ALIASES.some((pair) => pair.includes(a.toLowerCase()) && pair.includes(b.toLowerCase()));

/** The name a destination is offered for `file` named `name` (`exact_plan::offered_drag_name`). */
export function offeredName(file, name) {
  const theirs = suffix(file.slice(file.lastIndexOf('/') + 1)), mine = suffix(name);
  return theirs && !(mine && sameType(mine, theirs)) ? `${name}.${theirs}` : name;
}

/** The files and names `data-datatransferentries` holds (`exact_plan::decode_drag_entries`), or null. */
export function decodeEntries(value) {
  const mark = value.indexOf(NAMES_MARK);
  const entries = (part) => {
    if (part === '') return [];
    if (!part.endsWith('\n')) return null;
    try { return part.slice(0, -1).split('\n').map(decodeURIComponent); } catch { return null; }
  };
  const files = entries(mark < 0 ? value : value.slice(0, mark)), names = mark < 0 ? undefined : entries(value.slice(mark + 1));
  return files && names !== null ? { files, names } : null;
}

/** The files the page carries with the names they are offered under, a refused file taking its name with it. */
export function admitEntries(value, refuse) {
  const decoded = decodeEntries(value);
  if (!decoded) {
    refuse("the drag's files could not be read: rebuild the app with this Exact");
    return { files: '', names: '' };
  }
  const { files, names = [] } = decoded, kept = [], offered = [];
  files.forEach((file, at) => {
    if (!file) return;
    if (/[\r\n]/.test(file)) return refuse(`${JSON.stringify(file)}: a file with a line break cannot be carried`);
    if (!admit(file, refuse)) return;
    const name = names[at] ?? '';
    if (!name) return void (kept.push(file), offered.push(''));
    const as = offeredName(file, name), why = nameRefusal(name) ?? nameRefusal(as);
    if (why) refuse(`name ${JSON.stringify(name)}: ${why}`);
    kept.push(file);
    offered.push(why ? '' : as);
  });
  for (const name of names.slice(files.length)) refuse(`name ${JSON.stringify(name)}: there is no file in files= for it`);
  return { files: kept.join('\n'), names: offered.some(Boolean) ? offered.join('\n') : '' };
}

/** The `url` the page carries, or '' with the refusal said. */
export function admitUrl(url, refuse) {
  const why = url ? httpUrlRefusal(url) : null;
  if (!why) return url ?? '';
  // The URL itself stays out of the journal: it may hold a password or a token.
  refuse(`url is not an absolute http: or https: URL: ${why}`);
  return '';
}

export function install({ PropHooks, say, document = globalThis.document, signal }) {
  const last = new WeakMap(), standing = new WeakMap();
  const write = (e, name, kept) => (kept ? e.setAttribute(name, kept) : e.removeAttribute(name));
  // Each element's refusals by attribute, journaled once while they stand
  const journaled = (e, name, run) => {
    const before = standing.get(e)?.[name] ?? [], now = [];
    const result = run((reason) => now.push(reason));
    for (const reason of now) if (!before.includes(reason)) say(`drag: refused: ${reason}`);
    (standing.get(e) ?? standing.set(e, {}).get(e))[name] = now;
    return result;
  };
  const hook = (name, run) => (PropHooks[name] = (e, v) => {
    const seen = last.get(e) ?? last.set(e, {}).get(e);
    if (seen[name] === v) return true;
    seen[name] = v;
    journaled(e, name, (refuse) => run(e, v, refuse));
    return true;
  });
  hook(FILES, (e, v, refuse) => write(e, FILES, v == null ? '' : admit(v, refuse)));
  hook(URL_PROP, (e, v, refuse) => write(e, URL_PROP, v == null ? '' : admitUrl(v, refuse)));
  hook(ENTRIES, (e, v, refuse) => {
    const kept = v == null ? { files: '', names: '' } : admitEntries(v, refuse);
    write(e, FILES, kept.files);
    write(e, NAMES, kept.names);
  });
  // A render on the server (main-server.js) admits the data and hears no drag.
  if (!document) return;
  // The listener keeps the app's `draggable` on the node, which a field's contact turns off for a time (drag-out.js).
  PropHooks.draggable = (e, v) => (writeDraggable(e, v), true);
  listen(document, (e) => e.$exactTree === true, { signal });
}
