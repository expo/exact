import { setLocale as applyLocale } from './chosen.js';
import { Chosen, locale } from './strings.js';
import { validTag } from './language-assets.js';

export { localeSlot } from './chosen.js';

/** Only a split browser plan imports this entry. The write and its persistence still run in one commit. */
export function setLocale(tag) {
  if (tag !== '' && !validTag(tag)) return applyLocale(tag);
  const name = locale(tag || null);
  Chosen.change(name, tag, resolved => {
    if (!tag && locale(null) !== resolved) return setLocale(tag);
    applyLocale(tag);
  });
}
