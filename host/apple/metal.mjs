// Apple's Windows Metal compiler under Wine produces the same AIR targets Exact's shaders need.
import { existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { resolve } from 'node:path';

export const metalToolchainInstructions = () => process.platform === 'linux'
  ? 'Install Wine and Metal Developer Tools for Windows, then set EXACT_APPLE_METAL_TOOLS to the extracted tools directory (README.md).'
  : 'Run `xcodebuild -downloadComponent MetalToolchain`.';

/** A configured Windows toolchain; null leaves the host's ordinary tool lookup in place. */
export function windowsMetalCommand(sdk, tool, args, options = {}, find = false) {
  const env = options.env ?? process.env;
  if (!env.EXACT_APPLE_METAL_TOOLS || !['metal', 'metallib'].includes(tool)) return null;
  if (!['iphoneos', 'iphonesimulator', 'macosx'].includes(sdk)) throw new Error(`host/apple: no Windows Metal target for ${sdk}`);
  const executable = resolve(env.EXACT_APPLE_METAL_TOOLS, 'bin', `${tool}.exe`);
  if (!existsSync(executable)) throw new Error(`host/apple: missing ${executable}; EXACT_APPLE_METAL_TOOLS names the extracted Metal Developer Tools directory`);
  if (find) return ['/usr/bin/printf', ['%s\n', executable], options];
  let minimum;
  const explicit = args.some(arg => ['-target', '--target'].includes(arg) || /^--?target=/.test(arg));
  const translated = args.flatMap(arg => {
    const match = /^-(mios-simulator-version-min|mios-version-min|mmacosx-version-min|mmacos-version-min)=(\d+(?:\.\d+){0,2})$/.exec(arg);
    if (tool !== 'metal' || !match) return [arg];
    if (minimum || explicit) throw new Error('host/apple: Metal compilation must declare one deployment target');
    const [, flag, version] = match;
    minimum = `air64-apple-${flag.includes('simulator') ? `ios${version}-simulator` : `${flag === 'mios-version-min' ? 'ios' : 'macos'}${version}`}`;
    return [];
  });
  const query = args.some(arg => ['--version', '--help'].includes(arg)) || (args.length === 1 && args[0] === '-v');
  if (tool === 'metal' && !explicit && !query) {
    translated.unshift('-target', minimum ?? `air64-apple-${sdk === 'iphoneos' ? 'ios17.0' : sdk === 'iphonesimulator' ? 'ios17.0-simulator' : 'macos14.0'}`);
  }
  const windows = path => `Z:${resolve(options.cwd ?? process.cwd(), path).replaceAll('/', '\\')}`;
  const paths = translated.map(arg => {
    if (/^(?:\/|\.\.?\/)/.test(arg)) return windows(arg);
    const match = /^(-I|-L|-fmodules-cache-path=)(\/.*)$/.exec(arg);
    return match ? match[1] + windows(match[2]) : arg;
  });
  const wine = env.EXACT_APPLE_WINE ?? (existsSync('/usr/lib/wine/wine64') ? '/usr/lib/wine/wine64' : 'wine');
  return [wine, [executable, ...paths], { ...options, env: { ...env, WINEDEBUG: env.WINEDEBUG ?? '-all',
    WINEPREFIX: env.WINEPREFIX ?? resolve(env.XDG_CACHE_HOME ?? resolve(env.HOME ?? homedir(), '.cache'), 'exact/windows-metal') } }];
}
