import {readFileSync, writeFileSync, mkdirSync} from 'node:fs';
import {resolve} from 'node:path';
import {decodePng} from '../png.mjs';
import {blind, comparePixels, compareTrees, externalElements, imageFacts, modelElements, stableFindings, verdict} from './compare.mjs';

export const json = (path, value) => writeFileSync(path, JSON.stringify(value, null, 2) + '\n');
export const STATE_FIELDS = ['slots', 'derives', 'resources', 'pending', 'queued', 'tasks', 'device', 'time', 'language', 'navigation', 'dialog', 'keyboard', 'focus', 'store', 'collections'];
// Diagnostics contain read counters, launch times and process identities, not application state.
export const modelState = state => Object.fromEntries(STATE_FIELDS.filter(k => k in (state ?? {})).map(k => [k, state[k]]));

export async function checkpoint(session, observer, directory) {
  mkdirSync(directory, {recursive: true});
  const findings = [], observations = [];
  async function read(name, source, fn) {
    const start = Date.now();
    try {
      const result = await fn();
      if (result?.error) throw new Error(result.error);
      const record = {source, identity: observer.identity, start, end: Date.now(), result};
      json(resolve(directory, `${name}.json`), record);
      observations.push({name, source, start, end: record.end});
      return result;
    } catch (error) {
      const record = {source, identity: observer.identity, start, end: Date.now(), error: error.message};
      json(resolve(directory, `${name}.json`), record);
      observations.push({...record, name});
      findings.push(blind(source, `${name}: ${error.message}`));
      return null;
    }
  }
  const settle = await read('settle', 'driver/clock', () => session.clock('settle'));
  if (settle?.settled === false) findings.push(blind('driver/clock', settle.reason ?? 'not settled'));
  const before = await read('inside-before', 'driver/tree', () => session.tree());
  const stateBefore = await read('state-before', 'driver/state', () => session.state());
  const outsideBefore = await read('outside-before', 'platform/accessibility', () => observer.tree());
  const frameBefore = await read('frame-before', 'platform/window-server', () => observer.capture(resolve(directory, 'outside-before.png'), outsideBefore));
  const insideFrame = await read('inside-frame', 'driver/screenshot-default', () => session.screenshot(resolve(directory, 'inside.png')));
  const windowFrame = await read('window-frame', 'driver/screenshot-window', () => session.screenshot(resolve(directory, 'inside-window.png'), true, session.host === 'ios' ? 'status' : undefined));
  const frameAfter = await read('frame-after', 'platform/window-server', () => observer.capture(resolve(directory, 'outside-after.png'), outsideBefore));
  const outsideAfter = await read('outside-after', 'platform/accessibility', () => observer.tree());
  const stateAfter = await read('state-after', 'driver/state', () => session.state());
  const after = await read('inside-after', 'driver/tree', () => session.tree());
  for (const tree of [outsideBefore, outsideAfter]) {
    if (!tree) continue;
    for (const error of tree.errors ?? []) findings.push(blind('platform/accessibility', error));
    if (!externalElements(tree).length) findings.push(blind('platform/accessibility', 'accessibility tree exposes no named elements'));
    if (tree.truncated) findings.push(blind('platform/accessibility', 'accessibility tree was truncated'));
    if (tree.frontmost === false) findings.push(blind('platform', 'target window is inactive'));
    if ((tree.elements ?? []).some(e => [e.label, e.value, e.identifier].some(v => typeof v === 'string' && v.length > 300 && v.endsWith('…'))))
      findings.push(blind('platform/accessibility', 'snapshot may contain truncated text'));
  }
  if (before && after) {
    findings.push(...stableFindings(before.nodes, after.nodes, 'driver/tree'));
    findings.push(...stableFindings([before.epoch, before.incarnation], [after.epoch, after.incarnation], 'driver/identity'));
  }
  if (stateBefore && stateAfter) findings.push(...stableFindings(modelState(stateBefore), modelState(stateAfter), 'driver/state'));
  if (outsideBefore && outsideAfter) findings.push(...stableFindings(outsideBefore, outsideAfter, 'platform/accessibility'));
  if (before?.nodes?.length === 0) findings.push(blind('driver/tree', 'empty model tree'));
  const treeComparison = before && outsideBefore ? compareTrees(modelElements(before), externalElements(outsideBefore)) : null;
  if (treeComparison) findings.push(...treeComparison.findings);
  const images = {}, facts = {};
  for (const [source, capture] of [['driver/screenshot-default', insideFrame], ['driver/screenshot-window', windowFrame]]) {
    if (capture?.moving || capture?.settled === false)
      findings.push(blind(source, capture.reason ?? 'capture reports unsettled motion'));
    if (capture?.pixelsWhy && !(capture.pixels === 'display' && capture.pixelsWhy === 'the display as it is, asked for'))
      findings.push(blind(source, capture.pixelsWhy));
  }
  for (const [name, capture] of Object.entries({outsideBefore: frameBefore, outsideAfter: frameAfter, inside: insideFrame, window: windowFrame})) {
    if (!capture) continue;
    const path = capture.path ?? capture.screenshot;
    try {
      const image = decodePng(readFileSync(path));
      images[name] = image;
      facts[name] = {...imageFacts(image), path};
      if (facts[name].uniform) findings.push(blind(name, 'uniform capture cannot witness app content'));
    } catch (error) { findings.push(blind(name, `unreadable capture: ${error.message}`)); }
  }
  if (facts.outsideBefore && facts.outsideAfter) {
    findings.push(...stableFindings(facts.outsideBefore.digest, facts.outsideAfter.digest, 'platform/window-server'));
    findings.push(...stableFindings(frameBefore.window, frameAfter.window, 'platform/window-identity'));
  }
  if (images.inside && images.outsideBefore) {
    const f = outsideBefore?.frame;
    const fullDisplay = session.host === 'ios' && insideFrame.pixels === 'display' && f?.[0] === 0 && f?.[1] === 0 &&
      insideFrame.w === f[2] && insideFrame.h === f[3] &&
      insideFrame.w * insideFrame.scale === images.inside.width && insideFrame.h * insideFrame.scale === images.inside.height;
    if (fullDisplay || images.inside.width !== images.outsideBefore.width || images.inside.height !== images.outsideBefore.height)
      findings.push(...comparePixels(images.inside, images.outsideBefore, 'default-vs-presented'));
    else findings.push(blind('default-vs-presented', 'equal dimensions alone do not establish viewport-to-window coordinates'));
  }
  if (images.window && images.outsideBefore) findings.push(...comparePixels(images.window, images.outsideBefore, 'window-vs-presented'));
  for (const capture of [insideFrame, windowFrame]) {
    if (capture?.notShown) findings.push({kind: 'driver-omission', detail: capture.notShown});
    if (capture?.imagesPending) findings.push(blind('driver/screenshot', `${capture.imagesPending} images still pending`));
    if (capture?.content === 'painted') findings.push({kind: 'capture-substitution', source: 'driver/screenshot-window', reason: 'driver supplied canvas readback pixels'});
  }
  if (frameBefore?.attachments?.length) findings.push(blind('window-vs-presented', 'attached windows captured separately; no compositing map was established'));
  const report = {status: verdict(findings), identity: observer.identity, observations, findings, images: facts, stateFields: STATE_FIELDS,
    treeComparison, limitation: 'Sampled stability only; no shared compositor transaction ID. XCTest omits zero-area and out-of-window elements. Anonymous structure and unaligned regions are not compared.'};
  json(resolve(directory, 'checkpoint.json'), report);
  return {report, model: before, state: stateBefore, outside: outsideBefore, outsideAfter};
}
