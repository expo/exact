# @exact/barcode-detection on devices, by hand

LLP 1133.027 §12 stage 2's hand checks: a real camera reading a real book's
barcode, which neither a simulator nor the agent's picture shows, and the cost
of scanning (D19). Each step says what to see; a step that shows something
else is a failure to report with a screenshot. Use any printed book with an
ISBN barcode on its back cover (an EAN-13 starting 978 or 979).

Shelf looks a scanned ISBN up at Open Library, so the device needs the
network.

## iPhone

Build: `bun host/apple/build.mjs shelf-apple --ios --device --run`.

1. Tap Add, then Scan Barcode. iOS asks for the camera with Shelf's purpose
   ("Reads the barcode on a book to fill in its title and author."). Allow.
   The back camera fills the viewfinder; the line under it reads "Point the
   camera at the barcode on the back of the book".
2. Hold the barcode 15 to 25 cm from the camera. Within about a second a
   spinner replaces the line, then the book's title and author show, with
   Use This Book.
3. Tap Use This Book: the add sheet's title and author are filled in, and
   the camera's indicator (the green dot) is gone.
4. Scan again and tap Cancel before the lookup ends: the add sheet returns
   and the green dot goes out within a second.
5. Scan a QR code or a supermarket barcode (not 978/979): the line keeps
   reading "Point the camera at the barcode on the back of the book";
   nothing is looked up.
6. Scan a book Open Library does not know (a recent small-press title):
   "No book found for ISBN 978…", with Try Again. Try Again returns to
   the hint, and the same book is not read again.
7. Cost (D19): attach Xcode to Shelf (Debug → Attach to Process) and
   read the Debug navigator's CPU and Energy Impact for one minute with the
   viewfinder open on a page with no barcode, then for one minute on the
   barcode. Report the model, the iOS version and the four readings; the
   scan's default interval is set from them.
8. Settings → Privacy & Security → Camera → Shelf off. Scan Barcode: the
   viewfinder stays dark and no scan runs; Cancel returns to the add sheet.

## Mac

Build: `bun host/apple/build.mjs shelf-apple --run`, on a Mac with a camera
(built in, or an iPhone through Continuity Camera).

1. As steps 1 to 4 above, with the Mac's camera. A built-in camera has a
   fixed focus far from a book; hold the barcode 30 to 40 cm away. macOS asks
   for the camera once.
2. With Continuity Camera, choose the iPhone in the camera menu if one is
   offered: the scan reads as on the phone.

## Android phone

The Kotlin implementation uses ML Kit's bundled barcode model (LLP 1133.027
D14). Its JVM tests cover attachment failures, a shared detector with per-scan
filters and history, frame cleanup, result mapping and upright image decoding.
QR and PDF417 inference, offline first use, concurrent scans, rotation, camera
release/resume and a photo during scanning passed on an Android 16 emulator.
The reporter confirmed QR and an Argentine DNI's PDF417 on a POCO X6 Pro
running Android 14, including rotation, Home/return and an offline launch
after a force-stop. Separate sources, photos and a fresh offline install
remain unqualified on that phone.

Build the APK (`bun scripts/agent-android.mjs build shelf --apk`; the NDK
and JDK 17 as `host/android/README.md` says), install it, and repeat the
iPhone steps on a Pixel. Step 8 is Settings → Apps → Shelf → Permissions →
Camera → Don't allow. Check the minified APK too: ML Kit's manifest-named
registrars must retain their no-argument constructors.

In an app that asks for QR and PDF417, scan both formats and then hide the
viewfinder. Confirm the scan becomes idle and the camera indicator goes
out, then reopen it and scan again. Rotate the phone while scanning and
check that results stay upright beside the viewfinder. Take a photo while
scanning. Repeat offline on a fresh install.
Under the Android agent, polls analyse the bitmap the viewfinder draws with
ML Kit and retain `seen` when the preferred picture changes. With no displayed
picture the scan is idle. JVM tests replace ML Kit with a detector stand-in;
run the agent picture drive on Android to qualify real inference on that path.
A refused photo is covered by a JVM CameraX stand-in that rejects the use-case
combination and verifies that preview and analysis remain bound.

## Chrome on Android, Safari on iPhone, a desktop browser

`bun host/web/dev.mjs --app shelf`, opened from the device at this Mac's LAN
address.

1. Chrome on Android (it has `BarcodeDetector`): as the iPhone steps 1 to 5.
2. Safari on iPhone and desktop Firefox have no `BarcodeDetector`: Shelf
   shows no Scan Barcode button.
3. Chrome on a Mac (it has `BarcodeDetector` through the system's
   detector): as steps 1 to 4, with the Mac's camera.
