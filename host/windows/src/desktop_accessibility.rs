//! One UI Automation hit in Window Management's desktop coordinates (LLP 1133.025 D1a).

use exact_painted::package::{json, Context, Json, Package, Reply};

#[cfg(windows)]
mod os;

/// A recorder's read-only desktop accessibility package.
pub struct DesktopAccessibility;

impl Package for DesktopAccessibility {
    fn new(_: Context) -> Self {
        Self
    }

    fn later(&mut self, name: &str, args: &Json, _: Option<&Json>, reply: Reply) {
        match name {
            "accessibilityStatus" | "requestAccessibility" => reply.send(json!({
                "available": cfg!(windows), "state": if cfg!(windows) { "granted" } else { "denied" }
            })),
            "elementAtPoint" => {
                let (Some(x), Some(y)) = (args["x"].as_f64(), args["y"].as_f64()) else {
                    return reply.error("TypeError", None);
                };
                #[cfg(windows)]
                {
                    let monitors = crate::displays::capture_snapshot().into_iter().map(|(screen, physical)| {
                        Monitor { css: screen.rect, scale: screen.scale, physical }
                    }).collect::<Vec<_>>();
                    let Some(point) = monitors.iter().find_map(|m| m.physical_point(x, y)) else {
                        return reply.error("NotFoundError", None);
                    };
                    // UIA can call into this process. Its client must run off the window thread in an MTA.
                    // Dropping the work if a thread cannot start aborts its Reply.
                    let _ = std::thread::Builder::new().spawn(move || match os::hit(point, &monitors) {
                        Ok(hit) => reply.send(hit),
                        Err(error) => reply.error(error, None),
                    });
                }
                #[cfg(not(windows))]
                {
                    let _ = (x, y);
                    reply.error("NotSupportedError", None);
                }
            }
            _ => reply.error("NotSupportedError", None),
        }
    }
}

#[cfg(any(windows, test))]
fn provider_role(roles: &str) -> Option<&'static str> {
    roles.split_ascii_whitespace().find_map(|word| {
        exact_kernel::ARIA_ROLES
            .iter()
            .copied()
            .find(|role| role.eq_ignore_ascii_case(word))
    })
}

#[cfg(any(windows, test))]
fn owner(pid: i32, process: Option<&str>, description: Option<&str>) -> Json {
    let stem = process
        .and_then(|file| std::path::Path::new(file).file_stem())
        .and_then(|s| s.to_str());
    let mut app = json!({"pid": pid, "name": description.or(stem).unwrap_or_default()});
    if let Some(file) = process {
        app["processName"] = json!(file);
    }
    app
}

#[cfg(any(windows, test))]
struct Monitor {
    css: [f64; 4],
    scale: f64,
    physical: [i32; 4],
}

#[cfg(any(windows, test))]
impl Monitor {
    fn physical_point(&self, x: f64, y: f64) -> Option<[i32; 2]> {
        let [left, top, width, height] = self.css;
        if self.physical[0] >= self.physical[2]
            || self.physical[1] >= self.physical[3]
            || !(left..left + width).contains(&x)
            || !(top..top + height).contains(&y)
        {
            return None;
        }
        let pixel = |at: f64, axis: usize| {
            let first = self.physical[axis];
            let last = self.physical[axis + 2] - 1;
            let px = f64::from(first) + (at - self.css[axis]) * self.scale;
            let nearest = px.round() as i32;
            // A pointer boundary can move a few ulps when its JSON number is decoded.
            let edge = self.css_axis(nearest, axis);
            let hit = if (at - edge).abs() <= 2.0 * f64::EPSILON * edge.abs() {
                nearest
            } else {
                px.floor() as i32
            };
            hit.clamp(first, last)
        };
        Some([pixel(x, 0), pixel(y, 1)])
    }

    fn css_axis(&self, coordinate: i32, axis: usize) -> f64 {
        crate::displays::screen_coordinate(
            self.css[axis],
            self.scale,
            coordinate,
            self.physical[axis],
        )
    }
}

#[cfg(any(windows, test))]
fn frame(rect: [i32; 4], monitors: &[Monitor]) -> Option<Json> {
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut intersecting = Vec::new();
    for monitor in monitors {
        let [l, t, r, b] = monitor.physical;
        let clipped = [
            rect[0].max(l),
            rect[1].max(t),
            rect[2].min(r),
            rect[3].min(b),
        ];
        if clipped[0] >= clipped[2] || clipped[1] >= clipped[3] {
            continue;
        }
        intersecting.push(monitor);
        for axis in 0..2 {
            bounds[axis] = bounds[axis].min(monitor.css_axis(clipped[axis], axis));
            bounds[axis + 2] = bounds[axis + 2].max(monitor.css_axis(clipped[axis + 2], axis));
        }
    }
    if !bounds[0].is_finite() {
        return None;
    }
    // Keep the offscreen part of a control. At a corner beyond the desktop,
    // use the nearest display on which the control actually appears.
    for (x, inside_x) in [(rect[0], rect[0]), (rect[2], rect[2] - 1)] {
        for (y, inside_y) in [(rect[1], rect[1]), (rect[3], rect[3] - 1)] {
            let distance = |m: &Monitor| {
                let [l, t, r, b] = m.physical;
                let dx = i128::from(inside_x) - i128::from(inside_x.clamp(l, r - 1));
                let dy = i128::from(inside_y) - i128::from(inside_y.clamp(t, b - 1));
                dx * dx + dy * dy
            };
            let nearest = intersecting.iter().min_by_key(|m| distance(m))?;
            for (axis, at) in [x, y].into_iter().enumerate() {
                let v = nearest.css_axis(at, axis);
                bounds[axis] = bounds[axis].min(v);
                bounds[axis + 2] = bounds[axis + 2].max(v);
            }
        }
    }
    let [left, top, right, bottom] = bounds;
    Some(json!({"x":left,"y":top,"width":right-left,"height":bottom-top}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_identity_keeps_the_pid_when_the_image_is_unavailable() {
        assert_eq!(owner(42, None, None), json!({"pid":42,"name":""}));
        assert_eq!(
            owner(42, Some("mspaint.exe"), Some("Paint")),
            json!({"pid":42,"name":"Paint","processName":"mspaint.exe"})
        );
        assert_eq!(owner(42, Some("tool.exe"), None)["name"], "tool");
    }

    #[test]
    fn the_first_recognized_provider_role_wins() {
        assert_eq!(provider_role("custom SWITCH heading"), Some("switch"));
        assert_eq!(provider_role("heading"), Some("heading"));
        assert_eq!(provider_role("window custom"), None);
    }

    #[test]
    fn a_pointer_on_a_fractional_scale_display_hits_the_same_pixel() {
        use crate::displays;
        let monitors = [
            displays::Monitor {
                rect: [0, 0, 1920, 1080],
                scale: 1.,
                primary: true,
                ..Default::default()
            },
            displays::Monitor {
                rect: [-2560, 0, 0, 1440],
                scale: 1.5,
                ..Default::default()
            },
        ];
        let (screen, physical) = displays::capture_rects(&monitors, None).remove(0);
        let hit = Monitor {
            css: screen.rect,
            scale: screen.scale,
            physical,
        };
        for x in -2560..0 {
            let point = [x, 500];
            let [x, y] = displays::pointer(&monitors, None, 1, point, None)
                .unwrap()
                .at;
            let wire = serde_json::to_string(&[x, y]).unwrap();
            let [x, y]: [f64; 2] = serde_json::from_str(&wire).unwrap();
            assert_eq!(hit.physical_point(x, y), Some(point));
        }
    }

    #[test]
    fn a_hit_and_its_frame_follow_each_monitors_pointer_space() {
        let monitors = [
            Monitor {
                css: [-100., -20., 100., 80.],
                physical: [-150, -30, 0, 90],
                scale: 1.5,
            },
            Monitor {
                css: [0., 0., 100., 80.],
                physical: [0, 0, 100, 80],
                scale: 1.,
            },
        ];
        assert_eq!(monitors[0].physical_point(-2., 6.), Some([-3, 9]));
        assert_eq!(monitors[0].physical_point(-0.1, 6.), Some([-1, 9]));
        assert_eq!(monitors[0].physical_point(0., 6.), None);
        assert_eq!(monitors[1].physical_point(9.75, 10.), Some([9, 10]));
        assert_eq!(
            frame([90, 70, 115, 98], &monitors),
            Some(json!({"x":90.,"y":70.,"width":25.,"height":28.}))
        );
        assert_eq!(
            frame([-12, 6, 9, 18], &monitors),
            Some(json!({"x":-8.,"y":4.,"width":17.,"height":14.}))
        );
        let right = Monitor {
            css: [100., 0., 100., 100.],
            scale: 2.,
            physical: [100, 0, 300, 200],
        };
        let left = Monitor {
            css: [0., 0., 100., 100.],
            scale: 1.,
            physical: [0, 0, 100, 100],
        };
        assert_eq!(
            frame([80, 90, 99, 125], &[left, right]),
            Some(json!({"x":80.,"y":90.,"width":19.,"height":35.}))
        );
    }
}
