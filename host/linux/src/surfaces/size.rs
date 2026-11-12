//! A canvas's size given to its module before each bind (`gpu_size`): a
//! stamped bind may tick a new world, which frames itself for its canvas
//! before its first tick (`Game::first_frame`).
use super::*;

/// Give canvas `id` (view `view`, surface `name`) its laid-out size in
/// points, when layout has sized it and the module exports `gpu_size`.
pub(super) fn give<D: DataSource>(abi: &Abi, host: &mut Host<D>, view: u32, id: u32, name: &str) {
    let Some(f) = host.kernel().node(view).map(|n| n.frame) else {
        return;
    };
    if f.width <= 0. || f.height <= 0. {
        return;
    }
    // SAFETY: `gpu_size(u32, f32, f32) -> u32` as gpu/src/native.rs declares it.
    let Ok(size) = (unsafe {
        abi.library
            .get::<unsafe extern "C" fn(u32, f32, f32) -> u32>(b"gpu_size")
    }) else {
        return;
    };
    if unsafe { size(id, f.width, f.height) } != 0 {
        host.log(format!("surface {name}: {:?}", abi.error()));
    }
}
