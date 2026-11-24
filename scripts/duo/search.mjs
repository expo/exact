// LLP 1127.000 — drive UIKit's search and compact Back on the cover display.
import { resolve } from 'node:path';

export async function searchDuo({ root, compile, plans, open, pose, settle, check }) {
  compile('header-search', resolve(root, 'scripts/fixtures/search-levels.contract'));
  if (!plans['header-search']) return;
  const session = await open({ host: 'ios', app: 'duo-lab', plan: plans['header-search'], touch: 'platform', timing: 'platform' });
  const tap = async (name) => {
    check((await session.tap(name)).delivery === 'platform', `search: UIKit receives the tap on ${name}`);
    await settle(session);
  };
  try {
    await pose(session, 0);
    await session.type('app', '/take-query');
    await settle(session);
    await tap('search');
    await session.type('search', 'Fuji');
    await settle(session);
    const search = (await session.tree()).nodes.find((node) => node.props.testId === 'search');
    check(search && (await session.state()).focus.editor === search.id, 'search: the cover editor has focus');
    // How far the results are scrolled, from a row's place in the list: one that stays on screen, as UIKit
    // reuses the cell of a row scrolled out of view, and relative to the list, which a fold moves to another column
    const scrolled = async () => (await session.layout('results-list')).node.space.window.y - (await session.layout('tail-2')).node.space.window.y;
    const initial = await scrolled();
    // A person's short drag, held before it lifts so the list does not coast: the first result stays in view, as a tap
    // on one out of view would scroll it back first
    await session.tap('results-list', { drag: { from: [120, 220], dx: 0, dy: -40, over: 300, hold: 300 } });
    await settle(session);
    const offset = await scrolled();
    check(offset > initial + 1, 'search: the results scroll before pushing detail');
    // A drag can lower the keyboard; the editor takes focus again so that Back has a responder to restore
    await tap('search');
    check((await session.state()).focus.editor === search?.id, 'search: the editor has focus before the result opens');
    await tap('result-a');
    const detail = await session.state();
    check(detail.navigation.url === '/book/a' && detail.navigation.stack.length === 2, 'search: Home and its direct child are the only visits');
    const { w, h } = (await session.layout('book-a')).node.space.local;
    // A swipe back from the detail's edge, completed or returned while held
    const swipe = async (cancel) => {
      const distance = w * (cancel ? 0.28 : 0.65);
      await session.tap('book-a', { drag: {
        from: [5, h / 2], dx: distance, dy: 0, over: 400, hold: 500,
        during: [async () => {
          const held = await session.state();
          check(held.navigation.transition.interactive && held.slots.backs === (cancel ? 1 : 0), 'search: the held swipe owes no Back yet');
        }],
        ...(cancel ? { path: [{ dx: -distance, dy: 0, over: 400 }] } : {}),
      } });
      await settle(session);
      return session.state();
    };
    // The completed swipe comes first: UIKit resumes the field's editing on a pop only while no cancelled pop has
    // taken it, as a hand-built app shows
    const back = await swipe(false);
    check(back.navigation.url === '/' && back.slots.backs === 1 && back.slots.query === 'Fuji', 'search: a completed swipe keeps the query and reconciles Back once');
    check(back.focus.editor === search?.id, 'search: UIKit restores first responder as Back completes');
    check(Math.abs(await scrolled() - offset) <= 1, 'search: the results return to their folded position');
    await tap('result-a');
    const stayed = await swipe(true);
    check(stayed.navigation.url === '/book/a' && stayed.slots.backs === 1 && stayed.slots.query === 'Fuji', 'search: a cancelled swipe keeps the visit and owes no Back');
    await pose(session, 180);
    await settle(session);
    const unfolded = await session.state();
    check(unfolded.navigation.url === '/book/a' && unfolded.slots.query === 'Fuji', 'search: unfolding keeps the visit and query');
    check(Math.abs(await scrolled() - offset) <= 1, 'search: unfolding keeps the results scrolled');
    // The sidebar keeps the offset with less space above the results, so a person scrolls back to the first ones
    await session.tap('results-list', { drag: { from: [120, 200], dx: 0, dy: 80, over: 300, hold: 300 } });
    await settle(session);
    await tap('result-b');
    const expanded = await session.state();
    check(expanded.navigation.url === '/book/b' && expanded.slots.query === 'Fuji' && expanded.slots.cancelCount === 0,
      'search: the expanded detail leaves the sidebar results open');
    check(!expanded.keyboard.visible, 'search: the keyboard is down when the reader closes expanded search');
    const { aim, error } = await session.op({ op: 'tap', id: search.id, aim: { press: true } });
    if (error) throw new Error(error);
    const { elements } = await session.carrier.touches.ask({ op: 'tree' });
    const names = /^(?:search(?:bar)?)?(?:cancel|close|dismiss)(?:search|button)?$/i;
    const closeName = value => typeof value === 'string' && names.test(value.replace(/[^a-z]/gi, ''));
    const buttons = elements.filter(element => element.enabled && ['Button', 'ToolbarButton'].includes(element.type));
    const identified = buttons.filter(button => closeName(button.identifier));
    const closes = identified.length ? identified : buttons.filter(button => closeName(button.label));
    if (closes.length !== 1) throw new Error(`expanded search exposes ${closes.length} Close buttons`);
    const [x, y, width, height] = closes[0].frame;
    const point = [x + width / 2, y + height / 2];
    const [dx, dy] = point.map((value, index) => value - aim.point[index]);
    const screen = aim.fixed.map((value, index) => value + dx * aim.fixedAxes[0][index] + dy * aim.fixedAxes[1][index]);
    const closed = await session.carrier.touches.ask({ op: 'tap', point, screen, display: aim.display });
    check(closed.done === true, `search: UIKit receives the expanded X tap (${closed.error ?? 'done'})`);
    await settle(session);
    const after = await session.state();
    check(after.navigation.route === expanded.navigation.route && after.slots.cancelCount === 1 &&
      after.slots.emptyEdits === 1 && after.slots.query === '', 'search: expanded X dispatches cancel and one empty edit without changing detail');
  } catch (error) {
    check(false, `search: ${error.message}`);
  } finally {
    await session.close();
  }
}
