// Required-reason declarations belong to the bundle that contains the calling binary.
// https://developer.apple.com/documentation/bundleresources/describing-use-of-required-reason-api
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { spawnSync } from './toolchain.mjs';
import { plistFile } from './assets.mjs';

const TYPES = 'NSPrivacyAccessedAPITypes', TYPE = 'NSPrivacyAccessedAPIType', REASONS = 'NSPrivacyAccessedAPITypeReasons';
const PREFIX = 'NSPrivacyAccessedAPICategory';
// Approved reasons, including their restrictions:
// https://developer.apple.com/documentation/bundleresources/app-privacy-configuration/nsprivacyaccessedapitypes/nsprivacyaccessedapitype
const APPROVED_REASONS = {
  FileTimestamp: ['DDA9.1', 'C617.1', '3B52.1', '0A2A.1'],
  SystemBootTime: ['35F9.1', '8FFB.1', '3D61.1'],
  DiskSpace: ['85F4.1', 'E174.1', '7D9E.1', 'B728.1'],
  ActiveKeyboards: ['3EC4.1', '54BD.1'],
  UserDefaults: ['CA92.1', '1C8F.1', 'C56D.1', 'AC6B.1'],
};

const EXACT_REASONS = {
  // Rust std/ibex document operations, app storage, RasterSpool and the update store:
  // metadata in the app's containers (C617.1) or documents the person selected (3B52.1).
  FileTimestamp: ['C617.1', '3B52.1'],
  // CExactLaunch, text measurement, runner timers, animation and Hermes/std clocks:
  // intervals between app events and timers, never a device identifier (35F9.1).
  SystemBootTime: ['35F9.1'],
  // BuildInfo, launch measurements, shortcuts and wake registrations use app-owned keys (CA92.1).
  UserDefaults: ['CA92.1'],
  // TextAreaIOS selects the enabled emoji keyboard for an authored emoji input (54BD.1).
  ActiveKeyboards: ['54BD.1'],
};

function command(cmd, args, input) {
  const out = spawnSync(cmd, args, { input, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  if (out.status !== 0) throw new Error(`privacy manifest: ${cmd} failed: ${out.stderr || out.error}`);
  return out.stdout;
}

/** Capture absence too: a file added during compilation belongs to the next build. */
export function capturePrivacyManifest(path) {
  try { return { path, bytes: readFileSync(path).toString('base64') }; }
  catch (error) { if (error.code === 'ENOENT') return { path, bytes: null }; throw error; }
}

export function capturedPrivacyManifest({ bytes }) {
  return bytes === null ? {} : JSON.parse(command('plutil', ['-convert', 'json', '-o', '-', '-'], Buffer.from(bytes, 'base64')));
}

export function readPrivacyManifest(path) {
  return capturedPrivacyManifest(capturePrivacyManifest(path));
}

/** App-wide collection and tracking disclosures do not describe a framework or watch companion. */
export function privacyAPIManifest(captured) {
  return { [TYPES]: mergePrivacyManifests(capturedPrivacyManifest(captured))[TYPES] };
}

/** Union declarations without allowing an app's false/empty entries to erase a dependency's disclosure. */
export function mergePrivacyManifests(...manifests) {
  const apis = new Map(), data = new Map(), domains = new Set();
  let tracking;
  for (const manifest of manifests) {
    if (!manifest || typeof manifest !== 'object' || Array.isArray(manifest)) throw new Error('privacy manifest: expected a dictionary');
    for (const key of Object.keys(manifest)) if (![TYPES, 'NSPrivacyTracking', 'NSPrivacyTrackingDomains', 'NSPrivacyCollectedDataTypes'].includes(key)) throw new Error(`privacy manifest: unknown key ${key}`);
    if ('NSPrivacyTracking' in manifest) {
      if (typeof manifest.NSPrivacyTracking !== 'boolean') throw new Error('privacy manifest: NSPrivacyTracking must be a boolean');
      tracking = tracking || manifest.NSPrivacyTracking;
    }
    for (const key of [TYPES, 'NSPrivacyTrackingDomains', 'NSPrivacyCollectedDataTypes']) if (key in manifest && !Array.isArray(manifest[key])) throw new Error(`privacy manifest: ${key} must be an array`);
    for (const row of manifest[TYPES] ?? []) {
      const approved = Object.entries(APPROVED_REASONS).find(([type]) => row?.[TYPE] === PREFIX + type)?.[1];
      if (!approved || !Array.isArray(row[REASONS]) || !row[REASONS].length || row[REASONS].some(reason => !approved.includes(reason))) throw new Error('privacy manifest: invalid required-reason API declaration');
      const reasons = apis.get(row[TYPE]) ?? new Set();
      row[REASONS].forEach(reason => reasons.add(reason)); apis.set(row[TYPE], reasons);
    }
    for (const domain of manifest.NSPrivacyTrackingDomains ?? []) {
      if (typeof domain !== 'string' || !domain) throw new Error('privacy manifest: invalid tracking domain');
      domains.add(domain);
    }
    for (const row of manifest.NSPrivacyCollectedDataTypes ?? []) {
      const key = row?.NSPrivacyCollectedDataType, purposes = row?.NSPrivacyCollectedDataTypePurposes;
      if (typeof key !== 'string' || !Array.isArray(purposes) || purposes.some(p => typeof p !== 'string')
          || typeof row.NSPrivacyCollectedDataTypeLinked !== 'boolean' || typeof row.NSPrivacyCollectedDataTypeTracking !== 'boolean') throw new Error('privacy manifest: invalid collected-data declaration');
      const previous = data.get(key);
      data.set(key, { NSPrivacyCollectedDataType: key,
        NSPrivacyCollectedDataTypeLinked: !!previous?.NSPrivacyCollectedDataTypeLinked || row.NSPrivacyCollectedDataTypeLinked,
        NSPrivacyCollectedDataTypeTracking: !!previous?.NSPrivacyCollectedDataTypeTracking || row.NSPrivacyCollectedDataTypeTracking,
        NSPrivacyCollectedDataTypePurposes: [...new Set([...(previous?.NSPrivacyCollectedDataTypePurposes ?? []), ...purposes])].sort() });
    }
  }
  return { [TYPES]: [...apis].sort(([a], [b]) => a.localeCompare(b)).map(([type, reasons]) => ({ [TYPE]: type, [REASONS]: [...reasons].sort() })),
    ...(tracking !== undefined ? { NSPrivacyTracking: tracking } : {}),
    ...(domains.size ? { NSPrivacyTrackingDomains: [...domains].sort() } : {}),
    ...(data.size ? { NSPrivacyCollectedDataTypes: [...data.values()].sort((a, b) => a.NSPrivacyCollectedDataType.localeCompare(b.NSPrivacyCollectedDataType)) } : {}) };
}

/** Imports survive distribution stripping. ObjC selectors live in the binary, not nm's symbol table. */
export function binaryPrivacyAPIs(binary) {
  const symbols = command('nm', ['-u', binary]);
  // otool treats a trailing parenthesized name as an archive member (#234).
  const scratch = binary.endsWith(')') ? mkdtempSync(resolve(tmpdir(), 'exact-privacy-binary-')) : null;
  let objects;
  try {
    const inspected = scratch ? resolve(scratch, 'binary') : binary;
    if (scratch) copyFileSync(binary, inspected);
    objects = command('otool', ['-ov', inspected]);
  } finally { if (scratch) rmSync(scratch, { recursive: true, force: true }); }
  const selectors = new Set();
  let references = false;
  // otool resolves selector pointers (including chained fixups) into __objc_methname.
  // Ordinary C strings alone do not establish an API use.
  for (const line of objects.split('\n')) {
    if (line.startsWith('Contents of (')) references = /,__objc_selrefs\) section$/.test(line);
    else if (references) {
      const match = /^\s*0x[0-9a-f]+\s+(\S+)\s*$/i.exec(line);
      if (match) selectors.add(match[1]);
    }
  }
  const selector = name => selectors.has(name);
  const imported = pattern => pattern.test(symbols);
  const categories = [];
  if (imported(/\b_(?:stat|fstat|fstatat|lstat)(?:\$INODE64)?\b|NSFile(?:Creation|Modification)Date|NSURL(?:ContentModification|Creation)DateKey/)
      || imported(/FileAttributeKeyV(?:12creationDate|16modificationDate)|URLResourceKeyV(?:15creationDateKey|26contentModificationDateKey)|URLResourceValuesV(?:12creationDate|23contentModificationDate)/)
      || selector('fileModificationDate')) categories.push('FileTimestamp');
  if (imported(/\b_mach_absolute_time\b/) || selector('systemUptime')) categories.push('SystemBootTime');
  if (imported(/NSUserDefaults|CFPreferences(?:Copy|Set|App|Get|Synchronize)/)) categories.push('UserDefaults');
  if (selector('activeInputModes')) categories.push('ActiveKeyboards');
  if (imported(/\b_(?:statfs|fstatfs|statvfs|fstatvfs)(?:\$INODE64)?\b|NSURLVolume(?:Available|Total)Capacity|NSFileSystem(?:Free)?Size/)
      || imported(/(?:URLResourceKeyV|URLResourceValuesV)\d+volume(?:Available|Total)Capacity/)) categories.push('DiskSpace');
  // getattrlist can request either category; there is no truthful category/reason without auditing its attributes.
  if (imported(/\b_(?:getattrlist|fgetattrlist|getattrlistat|getattrlistbulk)\b/)) categories.push('AttributeList');
  return categories;
}

export function privacyManifestForBinaries(binaries, manifests = [], { mac = false, modules = false } = {}) {
  const categories = new Set(binaries.flatMap(binaryPrivacyAPIs));
  const merged = mergePrivacyManifests({ [TYPES]: [...categories].filter(c => EXACT_REASONS[c] && (!modules || c === 'FileTimestamp')).map(c => ({ [TYPE]: PREFIX + c, [REASONS]: EXACT_REASONS[c] })) }, ...manifests);
  for (const category of categories) if (modules && EXACT_REASONS[category] && !merged[TYPES].some(row => row[TYPE] === PREFIX + category)) throw new Error(`privacy manifest: a native module in ${binaries.join(', ')} must declare its ${category} reason`);
  // SQLite's filesystem-type/locking probes are not a disk-space reason. Do not invent E174.1 or 85F4.1.
  // Apple excludes macOS from required-reason reporting; macOS may still inspect filesystem types.
  if (!mac && categories.has('DiskSpace') && !merged[TYPES].some(row => row[TYPE] === PREFIX + 'DiskSpace')) throw new Error(`privacy manifest: ${binaries.join(', ')} uses disk-space APIs without an audited reason; remove the call or supply its truthful declaration`);
  if (!mac && categories.has('AttributeList') && !manifests.some(m => m[TYPES]?.some(row => ['FileTimestamp', 'DiskSpace'].some(c => row[TYPE] === PREFIX + c)))) throw new Error(`privacy manifest: audit getattrlist attributes in ${binaries.join(', ')} and supply their file-timestamp or disk-space reasons`);
  // getattrlist's attributes are not encoded in its import; the supplied audit names its categories.
  merged[TYPES] = merged[TYPES].filter(row => [...categories].some(c => row[TYPE] === PREFIX + c)
    || (categories.has('AttributeList') && ['FileTimestamp', 'DiskSpace'].some(c => row[TYPE] === PREFIX + c)));
  return merged;
}

/** `resources` is the owning bundle's resource directory, including a versioned Mac framework's Resources. */
export function writePrivacyManifest(resources, binaries, { manifests = [], apiManifests = [], mac = false, modules = false } = {}) {
  const output = resolve(resources, 'PrivacyInfo.xcprivacy');
  const manifest = privacyManifestForBinaries(binaries, [...manifests.map(capturedPrivacyManifest), ...apiManifests.map(privacyAPIManifest)], { mac, modules });
  mkdirSync(resources, { recursive: true });
  writeFileSync(output, plistFile(manifest));
  return output;
}
