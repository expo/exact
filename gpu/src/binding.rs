use crate::{json, Module};

impl Module {
    /// Bind a positional JSON array or named JSON object through the same typed
    /// bind operation. A named argument the object omits takes its default.
    /// Unknown names and malformed input leave the surface intact.
    pub fn bind_json(&mut self, id: u32, text: &str, at_ms: Option<f64>) -> bool {
        self.bind_text(id, text, at_ms, false)
    }

    /// Bind a named JSON object of the arguments that changed: one it omits
    /// keeps its value from the last accepted bind, so a host need not send a
    /// large unchanged argument again. A refusal leaves that bind in place.
    pub fn bind_changed_json(&mut self, id: u32, text: &str, at_ms: Option<f64>) -> bool {
        self.bind_text(id, text, at_ms, true)
    }

    fn bind_text(&mut self, id: u32, text: &str, at_ms: Option<f64>, changed: bool) -> bool {
        let result = (|| {
            let (names, values) = json::parse_bindings(text)?;
            let inst = self.instances.get(&id).ok_or("no such canvas")?;
            let mut fields = inst.surface.arguments();
            if changed {
                if names.is_none() {
                    return Err("a changed bind names its arguments".to_string());
                }
                if !inst.bound {
                    return Err("a changed bind follows an accepted bind".to_string());
                }
                for ((_, target), value) in fields.iter_mut().zip(&inst.inputs) {
                    *target = value.clone();
                }
            }
            let Some(names) = names else {
                if fields.is_empty() {
                    return Ok(values);
                }
                if values.len() > fields.len() {
                    return Err(format!(
                        "expected at most {} surface arguments, got {}",
                        fields.len(),
                        values.len()
                    ));
                }
                for ((_, target), value) in fields.iter_mut().zip(values) {
                    *target = value;
                }
                return Ok(fields.into_iter().map(|(_, value)| value).collect());
            };
            for (name, value) in names.iter().zip(values) {
                let Some((_, target)) = fields.iter_mut().find(|(field, _)| field == name) else {
                    return Err(format!("unknown surface argument `{name}`"));
                };
                *target = value;
            }
            Ok(fields.into_iter().map(|(_, value)| value).collect())
        })();
        match result {
            Ok(values) => self.bind(id, &values, at_ms),
            Err(error) => {
                self.error = error;
                false
            }
        }
    }

    /// The canvas's size in points ([`crate::Surface::size`]); hosts give it
    /// before each bind once layout has sized the canvas.
    pub fn size(&mut self, id: u32, width: f32, height: f32) -> bool {
        if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
            self.error = format!("size: {width}x{height} points is not a canvas size");
            return false;
        }
        self.settle(id);
        let Some(inst) = self.instances.get_mut(&id) else {
            self.error = "no such canvas".into();
            return false;
        };
        if inst.surface.size(width, height) {
            inst.dirty = true;
        }
        inst.drain();
        true
    }
}
