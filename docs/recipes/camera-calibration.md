# Recipe: camera calibration for an open track

Calibration is a package read over an open `@exact/media-capture` video track.
Keep a consumer such as `exact-camera` showing it. The package opens no camera
and takes no lease. Pass the track ID, not its stream ID.

The [complete Android and iOS code](../reference.md#a-camera-tracks-calibration)
is compiled from [@fixture/camera-calibration](../../packages/fixtures/camera-calibration/),
with its manifests. Add it by path:

```sh
bun add @fixture/camera-calibration@file:/absolute/path/to/exact/packages/fixtures/camera-calibration
```

Place this beside the viewfinder, passing the opened video track:

```contract
use getCameraCalibration, CameraCalibration from "@fixture/camera-calibration"

component Calibration
  props
    track: string
  resource calibration = getCameraCalibration(track=track) as shape CameraCalibration else empty()
  action read
    refresh calibration
  view
    column
      button press=read
        text "Read calibration"
      text calibration.reason
```

`intrinsicMatrix` contains nine row-major numbers. Keep `deviceId`,
`referenceFrame` and `referenceDimensions` with it. Unreported fields are empty
arrays; `reason` says why intrinsics are absent. It is empty when they are present.
Refresh after the viewfinder starts or the camera, format, zoom or focus changes.
The agent substitute returns `reason="agent"`.

**Android.** Read the bound CameraX `Camera` from `VideoSource.camera` through
`Camera2CameraInfo` (`androidx.camera:camera-camera2:1.6.2`). Read
`LENS_INTRINSIC_CALIBRATION`, `LENS_DISTORTION`,
`LENS_INFO_AVAILABLE_FOCAL_LENGTHS`, `SENSOR_INFO_PHYSICAL_SIZE`,
`SENSOR_INFO_PIXEL_ARRAY_SIZE` and `SENSOR_INFO_PRE_CORRECTION_ACTIVE_ARRAY_SIZE`.
Intrinsics are relative to the pre-correction active rectangle's top-left,
not the preview. Many devices report none. Focal lengths and sensor size are
millimetres; the available focal lengths are not the current frame's measurement.

**iOS.** Add an `AVCaptureVideoDataOutput` through
`ExactVideoSource.configure(stopped: true)`. Check
`isCameraIntrinsicMatrixDeliverySupported`, then enable
`isCameraIntrinsicMatrixDeliveryEnabled` before capture resumes. Read
`kCMSampleBufferAttachmentKey_CameraIntrinsicMatrix` from each sample: its
`matrix_float3x3` has padded, column-major storage. Coordinates are sample-buffer
pixels. The fixture shares one output for concurrent reads and removes it after
one frame. Its two-second frame deadline starts after capture resumes; a camera
change restarts pending reads and discards old callbacks.

`AVCameraCalibrationData` comes only with supported depth/photo delivery and its
required capture settings; a video matrix does not include its distortion tables
or physical sensor dimensions. The [lookup](../reference.md#a-camera-tracks-calibration)
gives those settings, coordinate transforms, platform sources and tests.
For a clip, retain calibration from the samples encoded, with timestamps and
crop/rotation/stabilization transforms. A one-shot read is a snapshot.
