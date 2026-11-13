import { test, expect } from 'bun:test';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { hostFixtureFile, glueDeclarations } from './glue-fixture.mjs';

// The wasm host's prop writer on an option, in Chrome. An option with no value attribute takes its label as its
// value (HTML, "the option element"), so a bound value that equals the label must still become the attribute, or
// a later label changes it; a cleared binding removes the attribute and the label is the value again.
const props = glueDeclarations(hostFixtureFile(new URL('../glue.js', import.meta.url), 'utf8'), ['writeValue', 'applyProps']);

test('a bound option value survives a change of its label and a cleared one gives way to it', async () => {
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage();
    const seen = await page.evaluate(props => {
      const heldValues = new WeakMap(), composing = new WeakSet(), scrollDocs = new Set(), host = {};
      const syncMedia = () => {}, syncMarkup = () => {}, settleValue = () => {}, valuedControl = () => false;
      const { applyProps } = new Function('heldValues', 'composing', 'scrollDocs', 'host', 'syncMedia', 'syncMarkup',
        'settleValue', 'valuedControl', `${props}; return { applyProps };`)(heldValues, composing, scrollDocs, host,
        syncMedia, syncMarkup, settleValue, valuedControl);
      const select = document.createElement('select'), a = document.createElement('option'), b = document.createElement('option');
      a.textContent = 'a'; b.textContent = 'b'; select.append(a, b); document.body.append(select);
      const read = () => ({ value: b.value, attribute: b.getAttribute('value') });
      applyProps(b, { value: 'b' }, []);
      b.textContent = 'c';
      const bound = read();
      applyProps(b, {}, ['value']);
      return { bound, cleared: read() };
    }, props);
    expect(seen).toEqual({ bound: { value: 'b', attribute: 'b' }, cleared: { value: 'c', attribute: null } });
  } finally { await browser.close(); }
});
