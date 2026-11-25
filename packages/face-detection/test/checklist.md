# Still-image face detection on devices

The portraits in `../fixture` come from a generated contact sheet of a fictional adult. `frontal.jpg`, `turned.jpg`, `raised.jpg` and `closed.jpg` are 400 × 480 pixel crops. `tilted.jpg` rotates the frontal portrait 24° counterclockwise within the original 400 × 480 frame. The person’s right eye is on the left of this unmirrored image and moves down. `two.jpg` places the frontal portrait beside a smaller turned portrait. `exif6.jpg` stores the frontal portrait rotated 90° counterclockwise with EXIF orientation 6. The empty 400 × 300 image comes from the barcode fixture.

The shared vision drive picks each file, sends `detectFaces` and reads the answer through Contract. Run `bun packages/barcode-detection/test/agent.mjs ios --sim <udid>` or `bun packages/barcode-detection/test/agent.mjs macos`. Run `ANDROID_SERIAL=<emulator-serial> bun packages/barcode-detection/test/agent.mjs android` for the same files on Android. The drive requires pose by default. Pass `--allow-missing-pose` on Apple systems where Vision cannot report all three angles. It builds a minified APK for that emulator’s ABI. From `host/android`, run `./gradlew :canvas:testDebugUnitTest --tests 'exact.facedetection.*'`. The JVM tests exercise the package with a stubbed ML Kit detector, including all eight EXIF transforms, independent pixel axes, limits and model failures.

To run the Vision request tests on macOS, compile the package against the same Swift interface as the host. Run these commands from the checkout root.

```sh
build=$(mktemp -d)
swiftc -parse-as-library -emit-module -emit-object -module-name ExactPackage host/apple/modules/ExactPackage.swift -emit-module-path "$build/ExactPackage.swiftmodule" -o "$build/ExactPackage.o"
swiftc -I "$build" "$build/ExactPackage.o" packages/face-detection/apple/FaceDetection.swift packages/face-detection/test/FaceRequests.swift -o "$build/face-requests"
"$build/face-requests"
```

On an iPhone test host, include the package’s `fixture` folder and `exact.json` in the test app’s resources and run the same `FaceRequests` entry point. It checks the frontal and EXIF eye positions on an iPhone and on macOS.

For a visitor photo, read the whole list to catch a second person. Check the box against the picture before cropping, check the pose the platform supplies, and apply the app’s blink threshold to each eye probability present. Vision can find both eyes with the eyelids closed.

To regenerate the three derived JPEGs, run this with Pillow in `packages/face-detection/fixture`.

```python
from PIL import Image
front = Image.open("frontal.jpg")
turned = Image.open("turned.jpg")
gray = (133, 133, 133)
front.rotate(24, Image.Resampling.BICUBIC, fillcolor=gray).save("tilted.jpg", quality=86)
pair = Image.new("RGB", (720, 480), gray)
pair.paste(front, (0, 0))
pair.paste(turned.resize((300, 360), Image.Resampling.BICUBIC), (410, 60))
pair.save("two.jpg", quality=85)
exif = Image.Exif()
exif[274] = 6
front.transpose(Image.Transpose.ROTATE_90).save("exif6.jpg", quality=95, exif=exif)
```

## Recorded run

The iPhone 17 Pro simulator ran iOS 26.5 (23F77). Vision used the CPU on simulator stages that offered it. The arm64 Android 16 emulator ran the R8-minified release APK in airplane mode from its first launch. Both used the package’s default options on the same eight files. Boxes are rounded to pixels in `x, y, width, height` order. Pose is `yaw, pitch, roll` in degrees. Android eyes are left, right. Apple omitted the `android` member on every face.

On the same Mac, eight revision 3 Vision requests read one decoded 400 × 480 frontal portrait. The largest distance between readings of the first point of either eye was **0.00 px** on macOS 27.0.1 and **91.93 px** on the iOS 26.5 simulator. For both the rectangle and landmark requests, `supportedComputeStageDevices` listed the Neural Engine, GPU and CPU on macOS, where Vision chose automatically. The simulator listed GPU and CPU. Automatic and explicit GPU landmark requests both returned Vision error 9 (“Could not create inference context”). The eight successful simulator reads used the CPU. In a separate 32-read CPU run, an eye point first left the face box on read 10. The request test’s containment checks passed in ten fresh simulator processes. Landmark positions were verified on macOS and Android. The iPhone device check is pending.

| Image | iOS box (px) | Android box (px) | iOS pose (°) | Android pose (°) | Android eyes |
| --- | --- | --- | --- | --- | --- |
| Frontal | 85, 129, 243, 243 | 104, 97, 213, 281 | 0.5, -3.5, -2.9 | 0.8, 2.3, 1.2 | 1.00, 1.00 |
| Turned left | 174, 131, 220, 220 | 128, 96, 217, 281 | 31.6, -1.7, -1.9 | 37.7, 1.2, 2.3 | 0.99, 0.99 |
| Tilted right | 89, 125, 243, 243 | 99, 92, 211, 277 | -2.6, -1.8, 18.3 | 0.8, 2.7, 24.8 | 0.99, 1.00 |
| Chin raised | 102, 86, 216, 216 | 107, 90, 210, 269 | -0.2, 24.8, 3.9 | 3.3, 28.0, 2.7 | 1.00, 1.00 |
| Eyes closed | 92, 135, 235, 235 | 108, 103, 208, 273 | 0.3, -6.5, -2.7 | 1.2, 3.4, 1.0 | 0.00, 0.01 |
| Two faces · larger | 85, 129, 244, 244 | 103, 98, 209, 279 | 0.6, -1.3, -0.2 | 0.4, 2.2, 1.2 | 1.00, 1.00 |
| Two faces · smaller | 539, 158, 172, 172 | 507, 131, 161, 212 | 41.1, -0.8, 1.6 | 37.3, 0.6, 2.1 | 1.00, 0.99 |
| No face | — | — | — | — | — |
| EXIF 6 | 85, 130, 243, 243 | 104, 99, 210, 281 | 0.3, -3.4, -3.1 | 0.5, 2.7, 1.1 | 1.00, 1.00 |

The models agreed on the direction of each deliberate movement. Their turned-head yaw estimates differed by 6.1° and their tilted-head roll estimates by 6.4° before rounding. Closed-eye pitch differed by 9.9°, frontal pitch by 5.8°, and EXIF pitch by 6.1°. At the visitor app’s 25° threshold, the raised chin passed on iOS and failed on Android.

Bundled `com.google.mlkit:face-detection:16.1.7` added **13,791,238 bytes** to this arm64-v8a APK. With the detector it was 57,061,294 bytes. Repacking the same Rust library, assets and reader options without the detector’s Kotlin, registration and Maven dependency produced 43,270,056 bytes. This is a signed APK size difference for one ABI. The model ships in the APK and detection needs no Play Services download.
