# Recipe: reading and writing NFC tags

`@exact/nfc` reads and writes NDEF tags (LLP 1133.035). See Tag This Book in `apps/shelf`.

1. **Declare** the grant and its purpose. A signed iOS build also needs the
   App ID's NFC Tag Reading capability.

   ```ts
   export const grants = 'device.nfc nfcPurpose';
   ```

2. **Read the status.** Show nothing NFC while `available` is false, which is
   every desktop and every browser but Chrome on Android. While `radio` is
   `off` (Android), offer `openNFCSettings()` instead.

3. **Scan from a press.** `scan(once=true)` answers when it ends: `read` with
   the reading's id, `stopped`, `cancelled`, `timeout` or `interrupted`. Each
   tag read is a `reading` on the root's outermost node, whatever started the
   scan.

4. **Show the scan sheet.** iOS displays `apple.alertMessage` in its sheet
   (`readerUI` is `system`). On Android and web, show that message and Cancel
   once `state` is `granted`.

5. **Records with bytes are data code's.** Contract gets each record's
   `recordType` and decoded `text`. `ndefRecords({ reading })` in `app.ts`
   answers the same records with their `data`, and `write` takes
   `Uint8Array` data there.

```text
use nfcStatus, scan, abortScan, reading, NFCStatus, NDEFScanEnd, NDEFReading from "@exact/nfc"

component App
  resource nfc = nfcStatus() as shape NFCStatus else empty()
  state lastTag = ""
  mutation scanned as shape NDEFScanEnd
  derive panelShown = nfc.readerUI == "app" and nfc.state == "granted" and pending(scanned)
  task panel key=panelShown
    after(1, syncPanel)
  action startScan
    send scanned = scan(once=true, apple.alertMessage="nfc.hold")
  action syncPanel
    if panelShown
      showModal("nfc-panel")
    else
      close("nfc-panel")
  action stop
    if pending(scanned)
      abortScan()
  action tagRead(r: NDEFReading)
    lastTag = r.id
  view
    main reading=tagRead
      if nfc.available
        button press=startScan
          text "Scan"
      dialog id="nfc-panel" aria-label="Ready to Scan" close=stop
        text "Hold your phone near the tag."
        button commandfor="nfc-panel" command="close"
          text "Cancel"
```

**A failure on iOS is the sheet's last line.** Pass the sentences as
`apple.errorMessages` (strings keys, by cause: `readOnly`, `tooSmall`,
`lost`, `notNdef`, `failed`, `unreadable`), and show your own alert only where `readerUI`
is `app`. A failed write's `code` is D3's error (`NetworkError`,
`NotSupportedError`, `AbortError`, …) and its message ends with the cause.

**Lock with care.** `makeReadOnly()` cannot be undone: confirm first.

**Driving it.** A drive or a test file can supply the tags.
Lead the web test with `prefer permission.nfc granted` and
`prefer extension nfc.tags=[{"id":"book","records":[]}]`. Start the scan,
then use `prefer extension nfc.field="book"` and `clock settle` before
checking what the app read. In an interactive drive,
`tap @t cancel` or `tap @t timeout` ends a held scan.
