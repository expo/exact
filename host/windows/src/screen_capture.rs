//! `@exact/screen-capture` on Windows (LLP 1178 piece 7): GDI stills,
//! with the same geometry as window management and no window activation.

use exact_painted::package::{json, Context, Json, Package, Reply};
use exact_painted::screen_capture::{Captures, Display, Failure, FixtureCapture};

#[cfg(windows)]
pub(crate) mod os;

/// The package's session instance, composed only by apps that import it.
pub struct ScreenCapture {
    context: Context,
    captures: Captures,
    fixture: FixtureCapture,
}

impl Package for ScreenCapture {
    fn new(context: Context) -> Self {
        Self {
            fixture: FixtureCapture::new(context.clone()),
            context,
            captures: Captures::default(),
        }
    }

    fn later(&mut self, name: &str, args: &Json, preferred: Option<&Json>, reply: Reply) {
        if self.context.agent() {
            return self.fixture.later(name, args, preferred, reply);
        }
        match name {
            "screenRecordingStatus" | "requestScreenRecording" => reply.send(json!({
                "available": cfg!(windows),
                "state": if cfg!(windows) { "granted" } else { "denied" },
            })),
            "captureScreens" => match snapshot(args) {
                Ok(displays) => self.captures.start(&self.context, args, displays, reply),
                Err((code, why)) => reply.error(code, Some(&why)),
            },
            _ => reply.error("NotSupportedError", None),
        }
    }

    fn preferred(&mut self, values: &Json, previous: &Json) {
        self.fixture.preferred(values, previous);
    }
    fn cancel(&mut self, request: u64) {
        self.fixture.cancel(request);
        self.captures.cancel(request);
    }
    fn destroy(&mut self) {
        self.fixture.destroy();
        self.captures = Captures::default();
    }
}

fn snapshot(args: &Json) -> Result<Vec<Display>, Failure> {
    if ["screen", "rect", "window", "cursor"]
        .iter()
        .any(|key| !args[*key].is_null())
    {
        return Err((
            "NotSupportedError",
            "selected capture is unavailable on Windows".into(),
        ));
    }
    #[cfg(windows)]
    {
        crate::displays::capture_snapshot()
            .into_iter()
            .map(|(screen, rect)| {
                let (width, height) = dimensions(rect)?;
                Ok(Display {
                    screen: screen.json(true),
                    frame: None,
                    width,
                    height,
                    read: Box::new(move || os::capture(rect, width, height)),
                })
            })
            .collect()
    }
    #[cfg(not(windows))]
    Err((
        "NotSupportedError",
        "this host has no Windows desktop".into(),
    ))
}

#[cfg(any(windows, test))]
fn dimensions([left, top, right, bottom]: [i32; 4]) -> Result<(u32, u32), Failure> {
    let dimension = |a, b| {
        u32::try_from(i64::from(b) - i64::from(a))
            .ok()
            .filter(|n| *n != 0)
            .ok_or_else(|| ("EncodingError", "invalid monitor rectangle".into()))
    };
    Ok((dimension(left, right)?, dimension(top, bottom)?))
}

#[cfg(any(windows, test))]
fn rgba(bytes: &mut [u8]) {
    for pixel in bytes.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
        // BI_RGB's fourth byte is unused, not alpha.
        pixel[3] = 255;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_selectors_cannot_fall_back_to_capturing_every_monitor() {
        for option in [
            json!({"screen":{}}),
            json!({"rect":{}}),
            json!({"window":"gone"}),
            json!({"cursor":"always"}),
            json!({"cursor":"never"}),
        ] {
            assert_eq!(snapshot(&option).err().unwrap().0, "NotSupportedError");
        }
    }

    #[test]
    fn gdi_rectangles_keep_negative_origins_and_physical_pixels() {
        for (rect, size) in [
            ([-3440, -1440, 0, 0], (3440, 1440)),
            ([2560, 0, 6000, 1440], (3440, 1440)),
            ([0, -2560, 1440, 0], (1440, 2560)),
        ] {
            assert_eq!(dimensions(rect).unwrap(), size);
        }
        assert!(dimensions([0, 0, 0, 1]).is_err());
        assert!(dimensions([10, 0, 5, 1]).is_err());
        let mut pixels = [1, 2, 3, 0, 255, 128, 0, 42];
        rgba(&mut pixels);
        assert_eq!(pixels, [3, 2, 1, 255, 0, 128, 255, 255]);
    }
}
