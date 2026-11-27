use crate::{json, Module, Value};

// Only complete arguments can be colour slots. A named rest argument is stored
// in the last input as a list of [name, value] pairs.
pub(crate) fn has_color(inputs: &[Value], rest: bool) -> bool {
    if inputs
        .iter()
        .any(|value| json::color_components(value).is_some())
    {
        return true;
    }
    let Some(Value::List(pairs)) = inputs.last().filter(|_| rest) else {
        return false;
    };
    pairs.iter().any(|pair| {
        let Value::List(fields) = pair else {
            return false;
        };
        fields.get(1).and_then(json::color_components).is_some()
    })
}

impl Module {
    /// Bind a positional JSON array or named JSON object through the same typed
    /// bind operation. A named argument the object omits takes its default.
    /// Unknown names require `Surface::rest_arguments`; refusals leave the
    /// surface intact.
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
            let accepts_rest = inst.surface.rest_arguments();
            let mut rest = Vec::new();
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
                if accepts_rest {
                    if let Some(Value::List(pairs)) = inst.inputs.get(fields.len()) {
                        rest = pairs.to_vec();
                    }
                }
            }
            if let Some(names) = names {
                for (name, value) in names.iter().zip(values) {
                    if let Some((_, target)) = fields.iter_mut().find(|(field, _)| field == name) {
                        *target = value;
                    } else if accepts_rest {
                        let pair = Value::list(vec![Value::str(name), value]);
                        if let Some(target) = rest.iter_mut().find(|item| {
                            matches!(item, Value::List(pair) if pair.first().and_then(Value::as_str) == Some(name))
                        }) {
                            *target = pair;
                        } else {
                            rest.push(pair);
                        }
                    } else {
                        return Err(format!("unknown surface argument `{name}`"));
                    }
                }
            } else {
                if fields.is_empty() && !accepts_rest {
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
            }
            let mut inputs: Vec<_> = fields.into_iter().map(|(_, value)| value).collect();
            if accepts_rest {
                inputs.push(Value::list(rest));
            }
            Ok(inputs)
        })();
        match result {
            Ok(values) => self.bind_inputs(id, &values, at_ms, true),
            Err(error) => {
                self.error = error;
                false
            }
        }
    }

    /// The canvas's size in points ([`crate::Surface::size`]); hosts give it
    /// before the first bind once layout has sized the canvas, and again when
    /// that size changes.
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
            inst.invalidate();
        }
        inst.drain();
        true
    }
}
