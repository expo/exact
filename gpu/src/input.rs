//! Device input inside a canvas that asks for it (LLP 1046.002 S1).

/// A pointer's contact phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerPhase {
    /// Contact began.
    Down,
    /// Position changed.
    Move,
    /// Contact ended.
    Up,
    /// The platform cancelled the contact.
    Cancel,
}

/// The web's pointer types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    /// A mouse.
    Mouse,
    /// A finger.
    Touch,
    /// A pen.
    Pen,
}

/// A sampled sensor an app asked for on a canvas (`startSensorInput`, LLP
/// 1133.013 D6), by its Generic Sensor name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorKind {
    /// `Accelerometer`: m/s², gravity included.
    Accelerometer,
    /// `LinearAccelerationSensor`: m/s², gravity removed.
    LinearAcceleration,
    /// `GravitySensor`: m/s².
    Gravity,
    /// `Gyroscope`: rad/s.
    Gyroscope,
    /// `Magnetometer`: µT.
    Magnetometer,
    /// `AbsoluteOrientationSensor`: a quaternion `[x, y, z, w]`.
    AbsoluteOrientation,
    /// `RelativeOrientationSensor`: a quaternion `[x, y, z, w]`.
    RelativeOrientation,
}

impl SensorKind {
    /// Every kind, in declaration order.
    pub const ALL: [SensorKind; 7] = [
        Self::Accelerometer,
        Self::LinearAcceleration,
        Self::Gravity,
        Self::Gyroscope,
        Self::Magnetometer,
        Self::AbsoluteOrientation,
        Self::RelativeOrientation,
    ];
    /// Its name in `startSensorInput`'s `sensor` option and on the host's wire.
    pub fn name(self) -> &'static str {
        match self {
            Self::Accelerometer => "accelerometer",
            Self::LinearAcceleration => "linearAcceleration",
            Self::Gravity => "gravity",
            Self::Gyroscope => "gyroscope",
            Self::Magnetometer => "magnetometer",
            Self::AbsoluteOrientation => "absoluteOrientation",
            Self::RelativeOrientation => "relativeOrientation",
        }
    }
    /// The kind [`Self::name`] names.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }
}

/// One device event, stamped by the host, with coordinates in canvas points.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    /// A physical key while the canvas has focus.
    Key {
        /// `KeyboardEvent.code`, such as `KeyW` or `Space`.
        code: String,
        /// `KeyboardEvent.key`, such as `w` or a space.
        key: String,
        /// Pressed rather than released.
        down: bool,
        /// The platform's repeat flag.
        repeat: bool,
        /// Host clock in milliseconds.
        at_ms: f64,
    },
    /// A raw pointer contact or motion.
    Pointer {
        /// Platform pointer identifier.
        id: u32,
        /// Contact phase.
        phase: PointerPhase,
        /// Horizontal point in the canvas.
        x: f32,
        /// Vertical point in the canvas.
        y: f32,
        /// Horizontal device motion since this pointer's previous event, in
        /// points. Unbounded by the canvas or screen edge while the pointer is
        /// locked (`data-pointer-lock`), when the position stays put.
        dx: f32,
        /// Vertical device motion since this pointer's previous event.
        dy: f32,
        /// Device kind.
        kind: PointerKind,
        /// The web's pressed-button bit mask.
        buttons: u32,
        /// Host clock in milliseconds.
        at_ms: f64,
    },
    /// A canvas child feeding a named surface action, in control-local points.
    Control {
        /// The child's declared action.
        name: String,
        /// Host contact identifier.
        id: u32,
        /// Contact phase.
        phase: PointerPhase,
        /// Horizontal point inside the control.
        x: f32,
        /// Vertical point inside the control.
        y: f32,
        /// Host clock in milliseconds.
        at_ms: f64,
    },
    /// A wheel at a point in the canvas.
    Wheel {
        /// Horizontal delta.
        dx: f32,
        /// Vertical delta.
        dy: f32,
        /// Horizontal point in the canvas.
        x: f32,
        /// Vertical point in the canvas.
        y: f32,
        /// Host clock in milliseconds.
        at_ms: f64,
    },
    /// A gamepad's whole state in the W3C Gamepad API's standard mapping,
    /// sent when a button or an axis changed.
    Pad {
        /// The host's device index.
        index: u32,
        /// False when the pad went away or the canvas lost focus.
        connected: bool,
        /// False for a pad the host cannot read in the standard mapping.
        standard: bool,
        /// Button values in `[0, 1]` by standard index (0 is the bottom face button).
        buttons: [f32; 17],
        /// Left x, left y, right x, right y in `[-1, 1]`; positive y is down.
        axes: [f32; 4],
        /// Host clock in milliseconds.
        at_ms: f64,
    },
    /// Focus left the canvas: whatever was held is released.
    Blur {
        /// Host clock in milliseconds.
        at_ms: f64,
    },
    /// Text the app posted to this canvas with Contract's `postMessage(text, surface)`;
    /// every one is delivered, in order, never coalesced.
    Message {
        /// The posted text.
        text: String,
        /// Host clock in milliseconds.
        at_ms: f64,
    },
    /// One sample of a sensor the app asked for on this canvas
    /// (`startSensorInput`), in the W3C's units and the frame it asked for.
    Sensor {
        /// The sensor sampled.
        sensor: SensorKind,
        /// x, y, z (and w for an orientation's quaternion); always finite.
        values: [f32; 4],
        /// Samples of this sensor lost before this one (the ring overflowed,
        /// or acquisition paused); 0 almost always.
        dropped: u32,
        /// Host input clock in milliseconds, when the sample was taken.
        at_ms: f64,
    },
    /// The app's input of this sensor ended (`stopSensorInput`, the canvas
    /// hidden or unmounting, the session ending): its last sample no longer
    /// stands.
    SensorEnded {
        /// The sensor whose input ended.
        sensor: SensorKind,
        /// Host input clock in milliseconds.
        at_ms: f64,
    },
}

impl crate::Module {
    /// Whether this canvas asks for raw device input.
    pub fn wants_input(&self, id: u32) -> bool {
        self.instances
            .get(&id)
            .is_some_and(|i| i.surface.wants_input())
    }

    /// Whether this canvas reads gamepads ([`crate::Surface::wants_pads`]).
    pub fn wants_pads(&self, id: u32) -> bool {
        self.instances
            .get(&id)
            .is_some_and(|i| i.surface.wants_input() && i.surface.wants_pads())
    }

    /// Deliver one device event; the canvas is dirty after it unless the
    /// surface says it changed nothing ([`crate::Surface::input_changed`]).
    pub fn input(&mut self, id: u32, event: &InputEvent) -> bool {
        self.settle(id);
        self.check_device();
        let Some(inst) = self.instances.get_mut(&id) else {
            return self.fail::<()>("no such canvas").is_some();
        };
        inst.surface.input(event);
        inst.drain();
        if let Some(crate::SurfaceError(e)) = inst.surface.take_error() {
            self.error = e;
            return false;
        }
        inst.dirty |= inst.surface.input_changed();
        true
    }

    /// Parse and deliver one ABI event; malformed input is refused by name.
    pub fn input_json(&mut self, id: u32, text: &str) -> bool {
        match crate::json::parse_input(text) {
            Ok(event) => self.input(id, &event),
            Err(error) => self.fail::<()>(error).is_some(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    /// Changes what it shows only for a message reading "show".
    struct Resting(bool);
    impl Surface for Resting {
        fn bind(&mut self, _: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
            Ok(())
        }
        fn render(
            &mut self,
            _: &Frame,
            _: &wgpu::Device,
            _: &wgpu::Queue,
            _: &mut wgpu::CommandEncoder,
            _: &wgpu::TextureView,
            _: wgpu::TextureFormat,
        ) -> bool {
            false
        }
        fn input(&mut self, event: &InputEvent) {
            self.0 = matches!(event, InputEvent::Message { text, .. } if text == "show");
        }
        fn input_changed(&self) -> bool {
            self.0
        }
    }
    /// The trait's default.
    struct Plain;
    impl Surface for Plain {
        fn bind(&mut self, _: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
            Ok(())
        }
        fn render(
            &mut self,
            _: &Frame,
            _: &wgpu::Device,
            _: &wgpu::Queue,
            _: &mut wgpu::CommandEncoder,
            _: &wgpu::TextureView,
            _: wgpu::TextureFormat,
        ) -> bool {
            false
        }
    }
    static REGISTRY: Registry = Registry {
        surfaces: &[
            ("resting", 0, || Box::new(Resting(false))),
            ("plain", 0, || Box::new(Plain)),
        ],
        shaders: &[],
    };

    #[test]
    fn an_input_that_changed_nothing_leaves_the_canvas_as_it_was() {
        let mut m = Module::new(&REGISTRY);
        let message = |text: &str| InputEvent::Message {
            text: text.into(),
            at_ms: 0.,
        };
        let sample = InputEvent::Sensor {
            sensor: SensorKind::Gravity,
            values: [0., -9.8, 0., 0.],
            dropped: 0,
            at_ms: 0.,
        };
        let resting = m.create_headless("resting").unwrap();
        let plain = m.create_headless("plain").unwrap();
        for id in [resting, plain] {
            assert!(m.bind(id, &[], None));
            m.instances.get_mut(&id).unwrap().dirty = false;
        }
        let dirty = |m: &Module, id| m.instances[&id].dirty;

        assert!(m.input(resting, &sample));
        assert!(!dirty(&m, resting), "an ignored sample draws nothing");
        assert!(m.input(resting, &message("show")));
        assert!(dirty(&m, resting));
        assert!(m.input(resting, &sample));
        assert!(dirty(&m, resting), "a frame already owed stays owed");

        assert!(m.input(plain, &sample));
        assert!(dirty(&m, plain), "by default every input draws");
    }
}
