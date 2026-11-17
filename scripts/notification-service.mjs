import { createHash } from 'node:crypto';
import { lstatSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { isAbsolute, relative, resolve, sep } from 'node:path';
import { TextDecoder } from 'node:util';

export const notificationServiceSourceRoot = 'modules/apple/notification-service';
export const ignoredNotificationServiceEntry = name => name.startsWith('.') || name === 'target';
const slash = path => path.split(sep).join('/');
const order = (a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b));
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const utf8 = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });

function sourceError(name, message) {
  return Object.assign(new Error(`${name}: ${message}`), { source: name });
}

const serviceDirectory = appDir => resolve(appDir, notificationServiceSourceRoot);
const serviceAppDirectory = directory => resolve(directory, '../../..');

/** The convention path and each of its app-owned ancestors must be real
 * directories. This check precedes every attempt to enumerate the service. */
export function notificationServiceDirectoryExists(appDir) {
  try { if (!statSync(serviceDirectory(appDir)).isDirectory()) return false; }
  catch (error) {
    if (error.code === 'ENOENT' || error.code === 'ENOTDIR' || error.code === 'ELOOP') return false;
    throw error;
  }
  for (const name of ['modules', 'modules/apple', notificationServiceSourceRoot]) {
    const path = resolve(appDir, name);
    let metadata;
    try { metadata = lstatSync(path); }
    catch (error) { if (error.code === 'ENOENT' || error.code === 'ENOTDIR') return false; throw error; }
    if (metadata.isSymbolicLink()) throw sourceError(name, 'is a symbolic link; notification service sources cannot be symbolic links');
    if (!metadata.isDirectory()) return false;
  }
  return true;
}

/** The membership check consumers use to notice additions to the bake's
 * recorded inventory. It reads names and types only; the bake owns identity. */
export function notificationServiceMembers(directory) {
  notificationServiceDirectoryExists(serviceAppDirectory(directory));
  const files = [];
  const visit = path => {
    for (const rawName of readdirSync(path, { encoding: 'buffer' })) {
      const raw = Buffer.isBuffer(rawName) ? rawName : Buffer.from(rawName);
      let entryName;
      try { entryName = utf8.decode(raw); }
      catch { throw sourceError(`${slash(relative(directory, path)) || '.'}/<${raw.toString('hex')}>`, 'has a non-UTF-8 source name'); }
      if (ignoredNotificationServiceEntry(entryName)) continue;
      const absolute = resolve(path, entryName), name = slash(relative(directory, absolute));
      const type = lstatSync(absolute);
      if (type.isSymbolicLink()) throw sourceError(name, 'is a symbolic link; notification service sources cannot be symbolic links');
      if (type.isDirectory()) visit(absolute);
      else if (type.isFile()) {
        if (!name.endsWith('.swift') && name !== 'PrivacyInfo.xcprivacy') {
          throw sourceError(name, 'is an unsupported notification service resource; use Swift sources and a top-level PrivacyInfo.xcprivacy');
        }
        files.push({ path: name, absolute });
      } else throw sourceError(name, 'is not a regular notification service source file');
    }
  };
  visit(directory);
  return files.sort((a, b) => order(a.path, b.path));
}

function recordedFiles(identity) {
  if (!identity || typeof identity.tree !== 'string' || !Array.isArray(identity.files)) throw new Error('the notification service bake identity is missing its recorded files');
  const files = identity.files.map((file, index) => {
    if (!file || typeof file !== 'object' || Array.isArray(file) || typeof file.path !== 'string' || !/^[0-9a-f]{64}$/.test(file.sha256)) {
      throw new Error(`the notification service bake identity has an invalid file at index ${index}`);
    }
    if (!file.path || isAbsolute(file.path) || file.path.split('/').some(part => !part || part === '.' || part === '..')
        || (!file.path.endsWith('.swift') && file.path !== 'PrivacyInfo.xcprivacy')) {
      throw new Error(`the notification service bake identity has an invalid path ${JSON.stringify(file.path)}`);
    }
    return file;
  });
  if (files.some((file, index) => index && order(files[index - 1].path, file.path) >= 0)) throw new Error('the notification service bake identity files are not in bytewise path order');
  return files;
}

function treeDigest(files) {
  const tree = createHash('sha256'), size = Buffer.alloc(8);
  for (const file of files) {
    size.writeBigUInt64LE(BigInt(file.bytes.length));
    tree.update(file.path).update(Buffer.from([0])).update(size).update(file.bytes);
  }
  return tree.digest('hex');
}

/** Verify the live bytes only at the names the bake recorded. The directory
 * listing exists to detect a new member; it does not produce identity. */
export function verifyNotificationServiceInventory(directory, identity) {
  const recorded = recordedFiles(identity), members = notificationServiceMembers(directory);
  const expected = recorded.map(file => file.path), actual = members.map(file => file.path);
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    const names = [...new Set([...expected.filter(name => !actual.includes(name)), ...actual.filter(name => !expected.includes(name))])];
    throw Object.assign(new Error(`the notification service sources changed after the bake recorded their identity${names.length ? ` (${names.join(', ')})` : ''}; build again`), { rebuild: true });
  }
  const files = recorded.map((file, index) => {
    const bytes = readFileSync(members[index].absolute);
    if (sha256(bytes) !== file.sha256) throw Object.assign(sourceError(file.path, 'changed after the bake recorded its digest; build again'), { rebuild: true });
    return { ...file, absolute: members[index].absolute, bytes };
  });
  if (treeDigest(files) !== identity.tree) throw new Error('the notification service bake identity tree does not match its recorded files');
  return files;
}

/** The verified inventory a completed Apple build records as a binary input. */
export function notificationServiceBuildInventory(appDir, identity, nameOf) {
  if (!identity) return [];
  const path = serviceDirectory(appDir);
  verifyNotificationServiceInventory(path, identity);
  return [{ kind: 'notification-service', name: nameOf(path), path, identity }];
}

/** Freshness and deploy comparisons name changed members and ignore entries
 * outside the convention. A malformed visible entry is itself the change. */
export function notificationServiceInputChanges(directory, identity) {
  try {
    const recorded = recordedFiles(identity), members = notificationServiceMembers(directory);
    const expected = new Map(recorded.map(file => [file.path, file])), actual = new Map(members.map(file => [file.path, file]));
    const changed = [...expected.keys()].filter(name => !actual.has(name));
    changed.push(...[...actual.keys()].filter(name => !expected.has(name)));
    const verified = [];
    for (const [name, file] of expected) if (actual.has(name)) {
      try { const bytes = readFileSync(actual.get(name).absolute); if (sha256(bytes) !== file.sha256) changed.push(name); else verified.push({ ...file, bytes }); }
      catch { changed.push(name); }
    }
    if (!changed.length && treeDigest(verified) !== identity.tree) return [notificationServiceSourceRoot];
    return [...new Set(changed)].sort(order);
  } catch (error) {
    return [error.source ?? notificationServiceSourceRoot];
  }
}

/** A private source tree whose bytes have already matched the bake. */
export function snapshotNotificationService(directory, identity, snapshot) {
  try {
    const files = verifyNotificationServiceInventory(directory, identity);
    for (const file of files) {
      const output = resolve(snapshot, file.path);
      mkdirSync(resolve(output, '..'), { recursive: true });
      writeFileSync(output, file.bytes);
    }
    return { directory: snapshot, files: files.map(file => ({ ...file, absolute: resolve(snapshot, file.path) })) };
  } catch (error) {
    rmSync(snapshot, { recursive: true, force: true });
    throw error;
  }
}
