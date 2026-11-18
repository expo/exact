//! A handle no mounted canvas's latest draw asks for is let go (LLP 1056
//! D9): the batch tells the presenter, which then decodes no change to it.
use super::{Bridge, Hooks};
use exact_plan::Value;
use exact_runner::exact_canvas::{Context2d, DrawError, Frame};
use exact_runner::{DataError, DataSource};
use std::sync::atomic::{AtomicBool, Ordering};

static ASKS: AtomicBool = AtomicBool::new(true);

struct Still;
impl DataSource for Still {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
    fn canvas_surfaces(&self) -> Vec<(String, usize)> {
        vec![("still".into(), 0)]
    }
    fn draw_2d(
        &mut self,
        _: &str,
        _: &[Value],
        ctx: &Context2d,
        _: &Frame,
    ) -> Result<bool, DrawError> {
        if ASKS.load(Ordering::Relaxed) {
            ctx.draw_image_with_image_handle("still.png", 0., 0.)
                .unwrap();
        }
        Ok(false)
    }
}

#[test]
fn a_handle_the_canvas_stops_asking_for_is_dropped() {
    let plan =
        contract::compile("component App\n  view\n    canvas surface=still() width=10 height=10\n")
            .unwrap()
            .encode();
    let mut bridge = Bridge::<Still>::new();
    let mut on = |f: &mut dyn FnMut(&mut Bridge<Still>) -> u32| {
        let roles = bridge.roles();
        let _roles = roles.as_ref().map(exact_kernel::style::roles::bind);
        let len = f(&mut bridge);
        let batch: serde_json::Value =
            serde_json::from_slice(bridge.output_bytes(len as usize)).unwrap();
        (
            batch["canvasImages"].clone(),
            batch["canvasImagesDropped"].clone(),
        )
    };
    on(&mut |b| b.canvas_display(1.0, (8u64 << 30) as f64));
    let (asked, dropped) = on(&mut |b| b.boot(&plan, Still, Hooks::none(), 100.0, 100.0));
    assert_eq!(
        (asked, dropped),
        (serde_json::json!(["still.png"]), serde_json::Value::Null)
    );
    ASKS.store(false, Ordering::Relaxed);
    let (asked, dropped) = on(&mut |b| b.canvas_display(2.0, (8u64 << 30) as f64));
    assert_eq!(
        (asked, dropped),
        (serde_json::Value::Null, serde_json::json!(["still.png"]))
    );
}
