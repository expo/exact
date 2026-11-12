# On the web, a barcode detection with `formats` unsaid rejects with a TypeError where `BarcodeDetector` reads no format

**Status:** Closed
**Resolution:** Web barcode detection uses refreshed supported formats, returns NotSupportedError when none can be read and preserves newer scans from older failures.
**Systems:** packages/barcode-detection/web/index.js
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

A browser can expose the `BarcodeDetector` interface and read no format. Chromium exposes it on Android, ChromeOS and macOS, and resolves `getSupportedFormats()` with an empty list when its barcode service cannot be reached ("Return an empty list to indicate that no barcode formats are supported", `barcode_detector_statics.cc`). On Android that is a device without Google Play services, or with an old one: `BarcodeDetectionProviderImpl.create()` returns null and the service's pipe is closed.

At `dd6f5dcd`, `detectorFor` (`packages/barcode-detection/web/index.js:63`) refuses only when the caller named formats the browser lacks (line 66, "this browser reads … no format"). With `formats` unsaid, `keep` is null, the formats to ask for are the browser's own list, which is empty, and line 69 runs `new Detector({ formats: [] })`. The Shape Detection API's constructor throws for that: "If barcodeDetectorOptions.formats is present and empty, then throw a new TypeError" (https://wicg.github.io/shape-detection-api/#barcode-detection-api); Chromium's message is "Hint option provided, but is empty."

`still` awaits `detectorFor` outside its `try` (line 83), and `analyse` awaits it in an async function with no `catch` around it (line 147), so `detectBarcodes` and `detectedBarcodes` reject with the browser's TypeError. The app sees a failure with no declared name, where `NotSupportedError` would have told it there is no detector (LLP 1133.027 D18).

## The failure

A stand-in `BarcodeDetector` whose `getSupportedFormats()` answers `[]` and whose constructor follows the specification:

```js
globalThis.BarcodeDetector = class {
  static async getSupportedFormats() { return []; }
  constructor(o) { if (o?.formats?.length === 0) throw new TypeError("Failed to construct 'BarcodeDetector': Hint option provided, but is empty."); }
  async detect() { return []; }
};
await impl.detectBarcodes({ image: 'app:/data/a.png', formats: ['qr_code'] });
await impl.detectBarcodes({ image: 'app:/data/a.png' });
```

Observed:

```
getSupportedFormats: {"formats":[],"tracks":false}
detectBarcodes, formats said:   answered {"error":"NotSupportedError","message":"this browser reads no format"}
detectBarcodes, formats unsaid: REJECTED TypeError: Failed to construct 'BarcodeDetector': Hint option provided, but is empty.
```

Expected: `{"error":"NotSupportedError"}` for both.

## Verification

Ran the unmodified module under Bun with that stand-in; the output above is its. `detectedBarcodes` over a stand-in video track rejected with the same TypeError. Not run in a browser in that state: the Chromium on the review Mac reads Vision's formats. That Chromium answers an empty list there, and throws for an empty `formats`, is from its source (`third_party/blink/renderer/modules/shapedetection/barcode_detector_statics.cc` and `barcode_detector.cc`; `services/shape_detection/android/java/…/BarcodeDetectionProviderImpl.java` and `InterfaceRegistrar.java`). Chrome on Windows and Linux is not such a browser: the interface is not exposed there (`runtime_enabled_features.json5`; LLP 1133.027 D18's table says the same).

## Direction

Answer `NotSupportedError` from `detectorFor` when the formats to ask for are empty, before a detector is built.

## Related

QUEUE "Android for vision and translation" is the unbuilt Android half. No issue covers the web's empty list.
