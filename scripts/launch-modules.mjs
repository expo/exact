// Native module folders and the launch parts app.json names, resolved to their sources.
import { existsSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

const ROOT = resolve(import.meta.dirname, '..');

/** The modules app.json `launch` names, in order, each from the app's
 * `modules/<name>/` else exact2's: `apple/launch/*.swift` links into the
 * executable, `apple/service/*.swift` builds its service dylib, `web` is
 * `web/service.js`, and each painted host has `<host>/launch.rs`.
 * Every declared host, plus any host the builder selects, must have a part. */
export function launchModules(dir, manifest, platforms = []) {
  const hosts = new Set([...Object.keys(manifest.host ?? {}), ...Object.keys(manifest.deploy?.store ?? {}), ...platforms]);
  return (manifest.launch ?? []).map((name) => {
    const own = resolve(dir, 'modules', name), shared = resolve(ROOT, 'modules', name);
    const base = existsSync(own) ? own : shared;
    if (!existsSync(base)) throw new Error(`app.json launch: no module ${name} (neither ${own} nor ${shared})`);
    const swift = (sub) => { const d = resolve(base, sub); return existsSync(d) ? readdirSync(d).filter(f => f.endsWith('.swift')).sort().map(f => resolve(d, f)) : []; };
    const file = sub => existsSync(resolve(base, sub)) ? resolve(base, sub) : null;
    const part = { name, launch: swift('apple/launch'), service: swift('apple/service'), web: file('web/service.js'), linux: file('linux/launch.rs'), android: file('android/launch.rs') };
    for (const host of hosts) {
      const apple = ['apple', 'ios', 'macos', 'tvos', 'watchos', 'visionos'].includes(host);
      if (apple ? !part.launch.length : !part[host]) {
        const path = apple ? 'apple/launch/*.swift' : host === 'web' ? 'web/service.js' : `${host}/launch.rs`;
        throw new Error(`app.json launch: ${name} has no ${host} launch part (${resolve(base, path)}); provide it or remove ${name} from launch`);
      }
    }
    return part;
  });
}

/** Platform-specific source folder; shared Apple modules remain valid for existing apps. */
export function moduleDirectory(dir, platform) {
  const own = resolve(dir, platform, 'modules');
  return existsSync(own) ? own : resolve(dir, 'modules', ['ios', 'macos', 'tvos'].includes(platform) ? 'apple' : platform);
}
