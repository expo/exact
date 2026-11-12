# @exact/clipboard: what a person checks (LLP 1133.006 §8, D3, D5)

The fixture is `packages/clipboard/test/fixture`, made into an app by
`fixture.mjs` (`EXACT_CLIPBOARD_FIXTURE=<dir>` keeps it). Build it from the
commit under test. Each step says what you should see.

## macOS, the real pasteboard

Run the fixture with `EXACT_DEVICE=real` (`bun exact.mjs agent macos`
with that variable set, or a plain `bun exact.mjs mac` build, which has no
agent).

1. Press **png**. In Preview, File > New from Clipboard opens an 8 × 8 red
   image.
2. Press **apng**. New from Clipboard opens an 8 × 8 **green** image: frame
   0 of the animation, not its red default image.
3. Press **both**. Paste into Messages: the red image arrives; paste into
   TextEdit as plain text (Edit > Paste and Match Style): the URL arrives.
4. Press **text**. Paste into TextEdit: "Copied from the fixture".
5. Run `osascript -e 'clipboard info'` after step 1 and record the types
   listed (D5: what `writeObjects([NSImage])` writes, and `public.png`).

## iOS, on a phone

6. Press **png**, then paste into Notes: an 8 × 8 red image. No banner
   appears for the write (D5).

## The web: when a write may happen (D3)

7. In Safari and in Firefox, open the fixture's web build from a served
   origin and press **png**. Record whether it writes or rejects
   `NotAllowedError`, and the browser's version.
8. Repeat with a delay before the write: in `app.ts`, await a 1 s timer
   before `write`. Record whether each browser still writes.

## Windows, required before landing #196

Run in PowerShell in the owner's signed-in desktop session. An SSH
clipboard belongs to another session and cannot qualify desktop paste.
Record `git rev-parse HEAD`, the Windows build (`winver`), consumer versions,
each command's result, screenshots and saved pasted images. These are
instructions, not recorded Windows results.

### Prepare the fixture

From the Exact checkout containing the commit under test:

```powershell
$exactRoot = (Get-Location).Path
cargo test -p exact-windows --lib --no-fail-fast
bun test packages/clipboard/test/binding.test.mjs
$env:EXACT_CLIPBOARD_FIXTURE = Join-Path $env:TEMP 'exact-clipboard-196'
@'
import { makeFixture } from "./packages/clipboard/test/fixture.mjs";
console.log(makeFixture().app);
'@ | bun -
Set-Location $env:EXACT_CLIPBOARD_FIXTURE
bun exact.mjs update --with windows
```

Keep this fixture until the results are recorded. In its `app.contract`,
add these declarations immediately below `component Fixture`:

```contract
  state pasted = ""
  action copiedText
    copyText("copyText replacement")
  action changed(value)
    pasted = value
```

Add these children to the existing `column`, before its other children:

```contract
      button press=copiedText testId="command-text"
        text "copyText"
      input value=pasted input=changed testId="pasted"
```

The fixture's **png** and **both** buttons take their image from the `RED`
base64 constant in `app.ts`. To test a different file, set its full path in
`$env:CLIPBOARD_PNG` and run this from the fixture directory before rebuilding:

```powershell
@'
import {readFileSync,writeFileSync} from "node:fs";
const b64=readFileSync(process.env.CLIPBOARD_PNG).toString("base64");
const source=readFileSync("app.ts","utf8");
writeFileSync("app.ts",source.replace(/const RED = [^;]+;/,`const RED = "${b64}";`));
'@ | bun -
bun exact.mjs windows --run
```

Rebuilding closes the window opened by the preceding build. The **png**
button must answer `ok png`; **both** must answer `ok both`. They write the
file into the app's own `app:/tmp/copy` and pass its path through the package.
A refusal is displayed in the fixture. Use these files in turn:

- A small PNG with red upper-left and blue lower-right corners, and visibly
  different top and bottom rows. Record its exact dimensions.
- An RGBA PNG with opaque, half-transparent and fully transparent regions.
  Keep the original for pixel and alpha comparison.
- A full-display or noisy PNG whose encoded size is above 2 MiB, below
  64 MiB, and whose dimensions multiply to at most 64,000,000.
- The narrow interlaced APNG and linear-gamma gray below.

Generate the last two fixtures from the Exact checkout, with the following
Bun code (save the outputs only in the temporary fixture directory). The
APNG bytes are the portable regression fixture; they contain a separate
red default and a 2×2 green/blue frame at (1, 0) on a 4×2 canvas.

```javascript
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { deflateSync } from 'node:zlib';
const dir = process.env.EXACT_CLIPBOARD_FIXTURE;
const test = readFileSync('host/windows/src/clipboard/image_tests.rs', 'utf8');
const literal = test.split('fn narrow_interlaced_apng_')[1].match(/let source = \[([^\]]+)\]/s)[1];
writeFileSync(join(dir, 'narrow.png'), Buffer.from(literal.split(',').map(x => x.trim()).filter(Boolean).map(Number)));
const u32 = n => { const b = Buffer.alloc(4); b.writeUInt32BE(n); return b; };
const chunk = (type, data) => {
  const body = Buffer.concat([Buffer.from(type), data]);
  let crc = 0xffffffff;
  for (const b of body) {
    crc ^= b;
    for (let i = 0; i < 8; i++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
  }
  return Buffer.concat([u32(data.length), body, u32((crc ^ 0xffffffff) >>> 0)]);
};
writeFileSync(join(dir, 'linear.png'), Buffer.concat([
  Buffer.from([137,80,78,71,13,10,26,10]),
  chunk('IHDR', Buffer.concat([u32(1), u32(1), Buffer.from([8,6,0,0,0])])),
  chunk('gAMA', u32(100000)),
  chunk('IDAT', deflateSync(Buffer.from([0,128,128,128,255]))),
  chunk('IEND', Buffer.alloc(0)),
]));
```

### Check the windowed writer

1. With the corners image selected, press **png**. Paste into Paint, Word
   and a Chromium image-paste editor. Check orientation, red/blue channels
   and original dimensions. Record which representation each consumer
   actually takes if its paste UI or diagnostics exposes that information;
   do not infer it solely from the app's name.
2. Repeat with the alpha image. In Word and an editor that preserves alpha,
   compare over light and dark backgrounds. Save a pasted PNG and inspect
   its alpha. Record black fills, dark fringes or lost transparency.
3. Use a clipboard inspector that can save the raw registered `PNG` and
   `CF_DIBV5` formats separately. PNG must be first among the offered image
   formats. The DIB has a 124-byte header, positive height, 32-bit bottom-up
   BGRA, `width * 4` stride, BI_BITFIELDS, RGBA masks at offsets 40–55,
   LCS_sRGB (`42 47 52 73`) at offset 56, and premultiplied alpha.
   PNG retains straight alpha. Read the synthesized `CF_DIB` and `CF_BITMAP`
   too. A byte check does not establish successful paste or synthesized
   alpha. To test the colour-space field, compare otherwise identical
   DIBV5 buffers and record the synthesized pixels.
4. Repeat with the image above 2 MiB. Check full dimensions and every edge,
   and note any visible pause. Decoding is synchronous on the host thread.
5. Press **both** with the corners image selected. Without copying again,
   paste the image into Paint and the URL into Notepad. The raw clipboard
   must hold PNG, DIBV5 and CF_UNICODETEXT together. Press **text** and paste
   into Notepad: `Copied from the fixture`. The image formats must be gone.
6. Select `narrow.png`, rebuild and press **png**. It must paste as a 4×2
   canvas: transparent outer columns, two green pixels in the top row and
   two blue pixels below them. Press the existing **apng** button too: its
   image must be 8×8 green, not the red default or a later blue frame.
7. Select `linear.png`, rebuild and press **png**. Only registered PNG is
   offered, with `gAMA=100000` and RGBA `(128,128,128,255)`, and no DIBV5.
   Compare the source and pasted PNG in a colour-managed viewer. A bitmap-only
   recipient may refuse this image; it must not receive falsely sRGB gray.
   Repeat with a PNG carrying non-sRGB cHRM or iCCP if available: the decoded
   colour information must survive, with no bitmap fallback. Text and EXIF
   metadata must not survive. HDR colour chunks the encoder cannot preserve
   reject `DataError` instead of losing their interpretation. Repeat with
   both iCCP and sRGB: iCCP wins under PNG third edition, so the profile
   survives, sRGB is removed, and no bitmap is offered.
8. After a successful copy, select a file containing `not a PNG`, rebuild
   and press **png**: `DataError`, prior clipboard unchanged. Repeat with an
   over-64-MiB file. Press the existing **large** button for a 9000×9000
   header: `DataError`, prior clipboard unchanged. **png-bytes** is deliberately
   unsupported on Windows; it must answer `NotSupportedError`.
9. Copy a valid image again, close the writer, and paste into Paint and a
   PNG-preferring editor. Reopen and repeat **both**. Eager clipboard data
   must remain pastable after the writer exits.

### Check agent startup and clipboard ownership

From the fixture directory, rebuild once after selecting the alpha image.
Leave the desktop clipboard's text as a sentinel and run:

```powershell
$env:EXACT_DEVICE = ''
$env:EXACT_AGENT_BIND = ''
Set-Clipboard 'exact-196 sentinel'
bun exact.mjs agent windows 'tap png' 'clock data' state 'tap command-text' 'type pasted key Control+v' tree logs
Get-Clipboard
```

The package state must describe a memory write. The `pasted` field must
contain `copyText replacement`, with no `copyText unsupported` diagnostic.
`Get-Clipboard` must still answer `exact-196 sentinel`. Package `lastWrite`
records the package call; it does not update for the later Contract command.
The Rust startup regression also verifies private contents are cleared when
the session ends.

Now enable real writes, once by each supported route:

```powershell
bun exact.mjs agent windows --bind clipboard 'tap both' 'clock data' state logs
$env:EXACT_DEVICE = 'real'
bun exact.mjs agent windows 'tap both' 'clock data' state logs
$env:EXACT_DEVICE = ''
```

After each command exits, paste into Paint, a PNG-preferring editor and
Notepad. Expect the image plus `https://example.com/red.png`, and a Windows
pasteboard in state, with no UnknownError about the window thread. The
headless host owns a message-only window; use a window inspector while a
persistent agent session is open to confirm its parent is HWND_MESSAGE and
it is invisible. Repeat each route with **command-text** followed by
`type pasted key Control+v`: the field and Notepad must contain
`copyText replacement`, and the image formats must be gone. Close/reopen
between runs. After a real write, leave a persistent agent idle and time
`Set-Clipboard` from a second process. Compare with no agent owning the
clipboard: replacement must not wait for the idle agent. Do not use synthetic
`type ... paste <text>` to test an actual clipboard read.

The [Windows formats documentation](https://learn.microsoft.com/windows/win32/dataxchg/clipboard-formats)
describes synthesized bitmap formats. [Chromium's clipboard writer](https://chromium.googlesource.com/chromium/src/+/main/ui/base/clipboard/clipboard_win.cc)
offers PNG first for consumers including Word. Neither establishes the
installed consumers' results; record the checks above before landing.
