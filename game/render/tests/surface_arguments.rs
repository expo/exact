//! A game's GPU module carries its surface's argument names, Unicode ones
//! included, as `exact_plan::surface_arguments` reads them back from the
//! product's bytes (expo/exact#176).
use exact_game::{Args, Game, Input, World};
#[derive(Default, Args)]
struct Options {
    énergie: f32,
    graines: u32,
}
struct Prairie;
impl Game for Prairie {
    const ID: &'static str = "surface-arguments";
    type Args = Options;
    fn setup(_: &mut World, _: &Options) {}
    fn tick(_: &mut World, _: &Input, _: &Options) {}
}
exact_game_render::module!(Prairie);
#[test]
fn the_module_record_reads_back_the_args_fields_as_written() {
    let found = exact_gpu::surface_arguments::declared(&EXACT_GPU_SURFACE_ARGUMENTS).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[Prairie::NAME], ["énergie", "graines"]);
}
