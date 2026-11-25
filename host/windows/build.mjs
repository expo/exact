#!/usr/bin/env bun
// The ordinary bake, packaged as a native executable, GPU DLL and baked assets.
//
//   bun host/windows/build.mjs <app> [--run] [--release] [--out <dir>]
//
// `--run` opens it. From SSH, which has no display a window can show on (docs/testing.md, "Windows: the
// NucBox"), it opens on the desktop of the user signed in at the console, through a one-off scheduled task, as
// host/windows/reference/draw.mjs opens WinUI's, and returns once the window is up.
import { createHash } from 'node:crypto';
import { copyFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, delimiter, dirname, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { bakeTarget, buildBake, copyShaders, developmentBuildEnv, resolveApp, windowsFile } from '../../scripts/app.mjs';
import { namesPlatform } from '../../scripts/app-shells.mjs';
import { guardFlags, reportBuilt } from '../../scripts/help.mjs';
import { unclaimedNote } from '../../scripts/links.mjs';
import { pickIcon, readPng, WINDOWS_ICON_SIZES, windowsIconRes } from '../../scripts/app-icons.mjs';
import { syncStaticTree } from '../web/serve.mjs';

const quote = (s) => `'${String(s).replaceAll("'", "''")}'`;
const powershell = (command) => spawnSync('powershell', ['-NoProfile', '-NonInteractive', '-Command', command], { encoding: 'utf8', windowsHide: true });

/** Why `app` has no Windows build, or null. A game builds from the SDK's shells; any other app names its hosts in
 * app.json, and an `exact new` app's `bun exact.mjs windows` names this one before it builds. */
export function noWindowsHost(app) {
  if (app.manifest.game || namesPlatform(app.manifest, 'windows')) return null;
  return `${app.name}'s app.json names no windows host, so it has no Windows build (apps/shelf/app.json names one)`;
}

/** Locate the shader compiler before Cargo can bake empty DXBC into an Ink binary. */
export function prepareShaders(manifest, env = process.env) {
  const painter = manifest?.host?.windows?.painter ?? (manifest?.game ? 'cpu' : 'ink');
  const onPath = String(env.PATH ?? '').split(delimiter).find(dir => dir && existsSync(resolve(dir, 'fxc.exe')));
  if (onPath) return;
  const bin = resolve(env['ProgramFiles(x86)'] ?? 'C:\\Program Files (x86)', 'Windows Kits', '10', 'bin');
  let versions = []; try { versions = readdirSync(bin).filter(v => /^\d+(\.\d+)+$/.test(v)); } catch {}
  const newest = versions.sort((a, b) => a.localeCompare(b, 'en', { numeric: true })).reverse().find(v => existsSync(resolve(bin, v, 'x64', 'fxc.exe')));
  if (newest) env.PATH = `${resolve(bin, newest, 'x64')}${delimiter}${env.PATH ?? ''}`;
  else if (painter !== 'cpu')
    throw new Error('Windows Ink requires fxc.exe: install the Windows SDK, or select host.windows.painter: "cpu".');
}

/** App-local Visual C++ runtime for the release target. The CRT directory in
 * VC/Redist contains Microsoft's redistributable DLLs, including those loaded
 * by a C++ dependency such as Hermes. See https://learn.microsoft.com/cpp/windows/redistributing-visual-cpp-files. */
export function windowsRuntime(target, env = process.env) {
  const arch = { x86_64: 'x64', aarch64: 'arm64', i686: 'x86' }[target.split('-')[0]];
  if (!arch || !target.endsWith('-windows-msvc')) throw new Error(`unsupported Windows runtime target ${target}`);
  const marker = `${arch}/Microsoft.VC*.CRT/vcruntime140.dll`;
  let paths = [];
  if (env.VCToolsRedistDir && existsSync(env.VCToolsRedistDir))
    paths = [...new Bun.Glob(marker).scanSync({ cwd: env.VCToolsRedistDir, absolute: true })];
  if (!paths.length) {
    const vswhere = resolve(env['ProgramFiles(x86)'] ?? 'C:\\Program Files (x86)', 'Microsoft Visual Studio', 'Installer', 'vswhere.exe');
    const found = spawnSync(vswhere, ['-products', '*', '-utf8', '-find', `VC/Redist/MSVC/*/${marker}`.replaceAll('/', '\\')], { encoding: 'utf8', windowsHide: true });
    if (found.status === 0) paths = found.stdout.trim().split(/\r?\n/).filter(Boolean);
  }
  if (!paths.length) throw new Error(`Windows release packaging needs the Visual C++ Redistributable files for ${arch}. Install the C++ redistributable component in Visual Studio Installer.`);
  const version = path => basename(resolve(dirname(path), '../..'));
  paths.sort((a, b) => version(b).localeCompare(version(a), 'en', { numeric: true }));
  const crt = dirname(paths[0]);
  return readdirSync(crt, { withFileTypes: true }).filter(f => f.isFile() && /\.dll$/i.test(f.name)).map(f => {
    const path = resolve(crt, f.name);
    return { path, sha256: createHash('sha256').update(readFileSync(path)).digest('hex') };
  });
}

/** Whether this process has no desktop of its own: an SSH session's windows are never shown. */
export const remote = () => !!process.env.SSH_CONNECTION;

/** The process `executable` that `--run` (or a window capture) last opened from its directory and that is still that
 * executable's: its PID, recorded in `<dir>/opened.json` when it opened, else null. */
function opened(executable) {
  let pid;
  try { pid = JSON.parse(readFileSync(resolve(dirname(executable), 'opened.json'), 'utf8')).pid; } catch { return null; }
  const path = powershell(`(Get-Process -Id ${Number(pid)} -ErrorAction SilentlyContinue).Path`).stdout.trim();
  return path.toLowerCase() === executable.toLowerCase() ? pid : null;
}

/** Ends the copy `--run` opened from `executable`'s directory, so the build can replace its files. */
export function closeOpened(executable) {
  const pid = opened(executable);
  if (pid) powershell(`Stop-Process -Id ${pid} -Force; Wait-Process -Id ${pid} -Timeout 10 -ErrorAction SilentlyContinue`);
  rmSync(resolve(dirname(executable), 'opened.json'), { force: true });
}

/** Opens `executable` on the console session's desktop, from its directory, and waits (at most 30 s) for its window.
 * With `shot`, the window's frame as the screen shows it (DWM's extended frame bounds: the title bar, Mica and the
 * content, no shadow) is written there as a PNG after `settle` ms, and the app is closed. Otherwise it keeps running
 * and its PID is recorded beside it (`closeOpened`). The PowerShell that does it runs headless (`conhost --headless`),
 * so no console window shows. The app's stderr goes to `opened.log` beside it. Returns `{ pid }` or `{ error }`. */
export async function openOnDesktop(executable, { shot = null, settle = 1500 } = {}) {
  const dir = dirname(executable), stamp = `${process.pid}-${Date.now()}`, result = resolve(tmpdir(), `exact-desktop-${stamp}.json`), log = resolve(dir, 'opened.log');
  const script = `$ErrorActionPreference = 'Stop'
try {
  Add-Type -AssemblyName System.Drawing
  Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public static class ExactDesktop {
  [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int cx, int cy, uint flags);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr window, int attribute, out Rect rect, int size);
}
'@
  [void][ExactDesktop]::SetProcessDPIAware()
  $p = Start-Process -FilePath ${quote(executable)} -WorkingDirectory ${quote(dir)} -RedirectStandardError ${quote(log)} -PassThru
  $deadline = (Get-Date).AddSeconds(30)
  while ($p.MainWindowHandle -eq 0 -and -not $p.HasExited -and (Get-Date) -lt $deadline) { Start-Sleep -Milliseconds 100; $p.Refresh() }
  if ($p.MainWindowHandle -eq 0) { throw "no window in 30 s$(if ($p.HasExited) { " (it exited: $((Get-Content ${quote(log)} -Tail 2) -join ' '))" })" }
  $out = @{ pid = $p.Id }
  ${shot ? `$w = $p.MainWindowHandle
  [void][ExactDesktop]::SetWindowPos($w, [IntPtr](-1), 0, 0, 0, 0, 3)
  $out.foreground = [ExactDesktop]::SetForegroundWindow($w)
  Start-Sleep -Milliseconds ${Number(settle)}
  $r = New-Object ExactDesktop+Rect
  [void][ExactDesktop]::DwmGetWindowAttribute($w, 9, [ref]$r, 16)
  $bitmap = New-Object System.Drawing.Bitmap ($r.Right - $r.Left), ($r.Bottom - $r.Top)
  $g = [System.Drawing.Graphics]::FromImage($bitmap)
  $g.CopyFromScreen($r.Left, $r.Top, 0, 0, $bitmap.Size)
  $bitmap.Save(${quote(shot)}, [System.Drawing.Imaging.ImageFormat]::Png)
  $out.size = @($bitmap.Width, $bitmap.Height)
  [void]$p.CloseMainWindow()
  if (-not $p.WaitForExit(5000)) { $p.Kill() }` : ''}
  $out | ConvertTo-Json -Compress | Set-Content -Encoding UTF8 ${quote(result)}
} catch { @{ error = "$_" } | ConvertTo-Json -Compress | Set-Content -Encoding UTF8 ${quote(result)} }`;
  const encoded = Buffer.from(script, 'utf16le').toString('base64'), task = `Exact-open-${stamp}`;
  const registered = powershell([
    `$a = New-ScheduledTaskAction -Execute 'conhost.exe' -Argument ${quote(`--headless powershell.exe -NoProfile -NonInteractive -EncodedCommand ${encoded}`)}`,
    `$p = New-ScheduledTaskPrincipal -UserId $env:USERNAME -LogonType Interactive`,
    `Register-ScheduledTask -TaskName ${quote(task)} -Action $a -Principal $p -Force | Out-Null`,
    `Start-ScheduledTask -TaskName ${quote(task)}`,
  ].join('; '));
  try {
    if (registered.status !== 0) return { error: `the scheduled task did not start: ${registered.stderr.trim().split('\n')[0]}` };
    for (const started = Date.now(); !existsSync(result) && Date.now() - started < 45_000;) await Bun.sleep(200);
    if (!existsSync(result)) return { error: 'nothing opened in 45 s: is someone signed in at the desktop? (query user names the console session)' };
    await Bun.sleep(100); // Set-Content's last write
    const said = JSON.parse(readFileSync(result, 'utf8').replace(/^﻿/, ''));
    if (said.pid && !shot) writeFileSync(resolve(dir, 'opened.json'), JSON.stringify({ pid: said.pid }) + '\n');
    return said;
  } finally {
    powershell(`Unregister-ScheduledTask -TaskName ${quote(task)} -Confirm:$false -ErrorAction SilentlyContinue`);
    rmSync(result, { force: true });
  }
}

/** Beside a packaged copy in `output`, the backend `build` was baked for and each executable and DLL it copied there,
 * by digest: the driver reads it there (agent-launch.mjs `refuseBinaryBackend`), as the bake cache keeps only the
 * latest build of the target and another package would replace it. */
export function writePackagedReceipt(app, build, output, runtime = []) {
  const products = build.products.filter((product) => /\.(exe|dll)$/.test(product.path)).concat(runtime)
    .map((product) => ({ path: windowsFile(app, product.path), sha256: product.sha256 }));
  writeFileSync(resolve(output, 'receipt.json'), JSON.stringify({ build: { backendOrigin: build.backendOrigin ?? null, products } }, null, 2) + '\n');
}

if (import.meta.main) {
  const args = guardFlags('windows', process.argv.slice(2));
  if (process.platform !== 'win32') throw new Error('build the Windows host on Windows with the MSVC Rust target');
  const outAt = args.indexOf('--out');
  if (outAt >= 0 && !args[outAt + 1]) throw new Error('--out needs a directory');
  const app = resolveApp(args.find((arg, i) => !arg.startsWith('--') && (outAt < 0 || i !== outAt + 1)));
  const unnamed = noWindowsHost(app);
  if (unnamed) { console.error(unnamed); process.exit(1); }
  // Every build packages into one directory, with one set of libraries and one compat.json.
  if (app.manifest.variant) throw new Error(`variant ${app.manifest.variant.name}: Windows takes no variant (LLP 1030.008 §6); build without --variant or EXACT_VARIANT`);
  prepareShaders(app.manifest);
  const target = bakeTarget('windows');
  const profile = args.includes('--release') ? 'release' : 'gpu-dev';
  const runtime = profile === 'release' ? windowsRuntime(target) : [];
  const trust = process.env.EXACT_UPDATE_TRUST ?? (profile === 'release' ? 'production' : 'development');
  // The executable's icon (LLP 1163 D6), which the app crate's build script links. Made again only
  // when the picked file changes, since a new file relinks the executable; its source's digest is kept beside it.
  let icon;
  try { icon = pickIcon(app.manifest, app.dir, { purposes: ['any', 'maskable'], px: 256, formats: ['png'], native: true, name: 'the Windows icon' }); }
  catch (error) { console.error(`${app.name}: ${error.message}`); process.exit(1); }
  const iconRes = icon ? resolve(process.env.EXACT_WINDOWS_OUTPUT ?? app.target, 'windows-icon.res') : '';
  if (icon) {
    const source = createHash('sha256').update(JSON.stringify(['res-v1', WINDOWS_ICON_SIZES])).update(readFileSync(icon.path)).digest('hex'), made = `${iconRes}.sha256`;
    if (!existsSync(iconRes) || !existsSync(made) || readFileSync(made, 'utf8') !== source) {
      mkdirSync(dirname(iconRes), { recursive: true });
      writeFileSync(iconRes, windowsIconRes(readPng(icon.path)));
      writeFileSync(made, source);
    }
  }
  // A running copy holds its executable and DLL open, so another directory lets
  // a second copy be packaged beside it.
  const output = outAt >= 0 ? resolve(args[outAt + 1]) : resolve(process.env.EXACT_WINDOWS_OUTPUT ?? resolve(app.dir, 'dist-windows'));
  const build = buildBake(app, 'windows', target, { profile, env: { ...developmentBuildEnv(), EXACT_UPDATE_TRUST: trust, EXACT_WINDOWS_ICON_RES: iconRes }, capture(build) {
    mkdirSync(output, { recursive: true });
    const exe = build.products.find(product => product.path.endsWith(`${app.crate('windows')}.exe`));
    if (!exe) throw new Error('Windows bake produced no executable');
    closeOpened(resolve(output, windowsFile(app, exe.path)));
    for (const product of build.products.filter(p => /\.(exe|dll)$/.test(p.path)).concat(runtime)) {
      const destination = resolve(output, windowsFile(app, product.path));
      try { copyFileSync(product.path, destination); }
      catch (error) {
        if (!['EBUSY', 'EPERM'].includes(error.code)) throw error;
        throw new Error(`${destination} is in use: quit it, or package into another directory with --out <dir> (drive that copy with EXACT_WINDOWS_BIN)`);
      }
      if (createHash('sha256').update(readFileSync(destination)).digest('hex') !== product.sha256) throw new Error(`Product changed before Windows capture: ${product.path}`);
    }
  } });
  mkdirSync(output, {recursive:true});
  const exe = build.products.find((product) => product.path.endsWith(`${app.crate('windows')}.exe`));
  if (!exe) throw new Error('Windows bake produced no executable');
  const executable = resolve(output, windowsFile(app, exe.path));
  for (const name of ['assets','deck']) {
    syncStaticTree(resolve(app.dir, name), resolve(output, name));
  }
  copyShaders(app, resolve(output, 'shaders'), {replace:true});
  writeFileSync(resolve(output, 'compat.json'), JSON.stringify(build.compat, null, 2) + '\n');
  writePackagedReceipt(app, build, output, runtime);
  console.log(`Windows: ${executable}`);
  const unclaimed = unclaimedNote(app.manifest, 'windows');
  if (unclaimed) console.log(`Windows: ${unclaimed}`);
  if (args.includes('--json')) reportBuilt({ path: executable, target: 'windows', trust: build.trust });
  if (args.includes('--run') && remote()) {
    const opened = await openOnDesktop(executable);
    if (opened.error) { console.error(`--run: ${opened.error}`); process.exitCode = 1; }
    else console.log(`opened on the desktop of the console session (pid ${opened.pid}; SSH shows no window); the next build here closes it`);
  } else if (args.includes('--run')) {
    const result = spawnSync(executable, [], {cwd:output, env:process.env, stdio:'inherit'});
    process.exitCode = result.status ?? 1;
  }
}
