use super::*;
#[derive(Default)]
struct Hook {
    seekable: bool,
    suspended: bool,
    unlocked: bool,
}
impl Executor for Hook {
    fn wants_audio(&self) -> bool {
        true
    }
    fn clock(&mut self, seekable: bool) {
        self.seekable = seekable;
    }
    fn suspend(&mut self, suspended: bool) {
        self.suspended = suspended;
    }
    fn unlock(&mut self) {
        self.unlocked = !self.seekable && !self.suspended;
    }
}
struct GameTest;
impl Game for GameTest {
    type Args = ();
    const NAME: &'static str = "lifecycle";
    const ID: &'static str = "test.lifecycle";
    fn setup(_: &mut World, _: &()) {}
    fn tick(_: &mut World, _: &exact_game::Input, _: &()) {}
}
#[test]
fn lifecycle_only_changes_presentation_and_clock_precedes_first_input() {
    let mut surface = WorldSurface::<GameTest, Hook>::default();
    surface.clock(false);
    surface.bind(&[], None).unwrap();
    assert_eq!(surface.messages(), ["exact:audio"]);
    assert!(surface.messages().is_empty());
    let saved = surface.carry();
    surface.input(&InputEvent::Key {
        code: "KeyW".into(),
        key: "w".into(),
        down: true,
        repeat: false,
        at_ms: 0.,
    });
    assert!(surface.presentation.unlocked);
    let saved_after_input = surface.carry();
    for seekable in [false, true] {
        surface.clock(seekable);
        surface.lifecycle(Lifecycle::Hidden);
        surface.lifecycle(Lifecycle::Interrupted);
        surface.lifecycle(Lifecycle::Visible);
        assert!(surface.presentation.suspended);
        surface.lifecycle(Lifecycle::Resumed);
        assert!(!surface.presentation.suspended);
        assert_eq!(surface.carry(), saved_after_input);
    }
    assert_ne!(saved, saved_after_input); // only the actual input changes saved state
}

#[test]
fn a_press_elsewhere_in_the_page_unlocks_audio_and_changes_no_state() {
    let mut surface = WorldSurface::<GameTest, Hook>::default();
    surface.clock(false);
    surface.bind(&[], None).unwrap();
    let saved = surface.carry();
    assert!(!surface.presentation.unlocked);
    surface.lifecycle(Lifecycle::Activated);
    assert!(surface.presentation.unlocked);
    assert!(!surface.presentation.suspended);
    assert_eq!(surface.carry(), saved);
}
