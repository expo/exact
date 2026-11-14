# transformImage refuses integral resize dimensions serialized from Rust floats

**Status:** Closed
**Resolution:** `transformImage` reads `resizeWidth` and `resizeHeight` as numbers and accepts any whole value from 1 to 16384, however its JSON is written, still refusing fractional and out-of-range sizes.
**Systems:** data/src/image/edit.rs, data/host/src/native.rs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At `f6be99af`, a Rust source passing `json!({"resizeWidth": 32.0})` to
`fs.transformImage` gets `transformImage: invalid: resizeWidth must be an
integer from 1 to 16384`. The dimension is integral and in range. Changing
only the Rust value's type to `32` makes the same request succeed. This
affects dimensions computed from the `f64` numbers used by Contract and
module bindings; `resizeHeight` has the same problem.

`parse_transform` at `data/src/image/edit.rs:159-165` uses
`serde_json::Value::as_u64`, which rejects a JSON number represented as a
float even when its value is integral. The Rust storage adapter copies the
option unchanged at `data/host/src/native.rs:230-259`. In contrast,
`compressImage` reads its numeric limits as `f64` and checks their numeric
value.

The minimal reproduction calls the production parser from `exact-data`:

```rust
use exact_data::image::edit::parse_transform;
use exact_data::modules::serde_json::json;

fn main() {
    for options in [
        json!({"resizeWidth": 32}),
        json!({"resizeWidth": 32.0}),
        json!({"resizeHeight": 32.0}),
    ] {
        println!("{options}: {:?}", parse_transform(&options));
    }
}
```

The first succeeds; the next two refuse. Also verified through
`image::encode_request` using the existing `oriented-gps.jpg` fixture:
the integer width produced a 32×43 JPEG, while both float requests refused
before decoding. The standalone reproducer and output are under
`/tmp/exact-review-20261112/image-resize-numeric/`; no product source or test
was changed.

Accept finite, integral numeric values in the documented range before
converting to `u32`. Check integer and float JSON representations for both
dimensions, while retaining the refusal of fractional or out-of-range sizes.

Searched open and closed filesystem issues and the cached GitHub issues;
none covers numeric representation in image resize options.
