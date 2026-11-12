//! Host-owned viewport answers; never baked or carried device data.
//! @ref LLP 1039 D1–D3; LLP 1061 D4; LLP 1078 D2
use super::{DataError, DataSource, Runner, RunnerError};
use crate::viewport::{Fold, Preferences, Viewport, SOURCE};
use exact_kernel::CommitReceipt;
use exact_plan::{TypeKind, Value};

impl<D: DataSource> Runner<D> {
    /// The current validated layout viewport.
    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    /// Re-answer every viewport resource in one commit. Invalid sizes are
    /// journaled without changing the fact or poisoning the runner. The
    /// display preferences stay as they were.
    pub fn set_viewport(
        &mut self,
        width: f64,
        height: f64,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let viewport = Viewport {
            width,
            height,
            ..self.viewport
        };
        if let Err(error) = viewport.validate() {
            let (width, height) = (exact_num::Shortest(width), exact_num::Shortest(height));
            self.log(format!("viewport {width} × {height} refused: {error:?}"));
            return Err(error);
        }
        self.replace_viewport(viewport, "viewport")
    }

    /// The user's display preferences changed (LLP 1061 D4): re-answer
    /// every viewport resource in one commit. A Bold Text change also commits
    /// when no resource reads the preferences, since it changes font metrics.
    pub fn set_preferences(
        &mut self,
        preferences: Preferences,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let viewport = Viewport {
            preferences,
            ..self.viewport
        };
        let bold = self.kernel.set_bold_text(preferences.bold_text);
        self.replace_fact(
            |r| &mut r.viewport,
            viewport,
            SOURCE,
            |r, _, readers| {
                let result = r.recommit(readers, "preferences").and_then(|receipt| {
                    if bold && receipt.is_none() {
                        r.commit_again(Vec::new(), "bold text").map(Some)
                    } else {
                        Ok(receipt)
                    }
                });
                if bold && result.is_err() {
                    let _ = r.kernel.set_bold_text(!preferences.bold_text);
                }
                result
            },
        )
    }

    /// The device's posture or the segments a fold makes changed (LLP 1078
    /// D2): re-answer every viewport resource in one commit; the same fold
    /// again, or no reader, commit nothing. A grid without a column or a
    /// row is refused before anything changes.
    pub fn set_fold(&mut self, fold: Fold) -> Result<Option<CommitReceipt>, RunnerError> {
        fold.validate()?;
        let viewport = Viewport {
            fold,
            ..self.viewport
        };
        self.replace_viewport(viewport, "fold")
    }

    fn replace_viewport(
        &mut self,
        viewport: Viewport,
        why: &str,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let geometry = crate::viewport::Geometry::of(&self.kernel);
        let changed = self.viewport_geometry != geometry;
        let previous = std::mem::replace(&mut self.viewport_geometry, geometry);
        let result = self.replace_fact(
            |r| &mut r.viewport,
            viewport,
            SOURCE,
            |r, _, mut readers| {
                if changed && readers.is_empty() {
                    readers = r
                        .resource_rows()
                        .enumerate()
                        .filter_map(|(i, row)| (r.plan.str(row.source) == SOURCE).then_some(i))
                        .collect();
                }
                r.recommit(readers, why)
            },
        );
        if result.is_err() {
            previous.restore(&mut self.kernel);
            self.viewport_geometry = previous;
        }
        result
    }

    /// Publish changed document insets or segment rectangles after the host
    /// updates the kernel environment. Unchanged geometry commits nothing.
    pub fn refresh_viewport_geometry(&mut self) -> Result<Option<CommitReceipt>, RunnerError> {
        self.replace_viewport(self.viewport, "viewport geometry")
    }

    pub(super) fn viewport_answer(&self, i: usize) -> Result<Value, DataError> {
        let row = self.resource_row(i);
        if row.args.len > 0 {
            return Err(DataError::BadArguments(format!(
                "{SOURCE} takes no arguments"
            )));
        }
        let ty = self.plan.type_(row.ty);
        if ty.kind != TypeKind::Record {
            return Err(DataError::Unavailable(format!(
                "{SOURCE} answers a record; this one is declared `{}`",
                self.plan.str(ty.name)
            )));
        }
        let mut fields = Vec::with_capacity(ty.fields.len as usize);
        for f in ty.fields.iter() {
            let name = self.plan.str(self.plan.field(f).name);
            let value = if name == "gpuTier" {
                Some(Value::str(self.page.gpu_tier()))
            } else if name == "accessibilityTextSize" {
                Some(Value::Bool(self.accessibility_text_size()))
            } else {
                self.viewport_geometry
                    .field(&self.plan, self.plan.field(f).ty, name, self.viewport)?
                    .or_else(|| self.viewport.field(name))
            };
            match value {
                Some(v) => fields.push(v),
                None => {
                    return Err(DataError::Unavailable(format!(
                        "{SOURCE} has no field `{name}`"
                    )))
                }
            }
        }
        Ok(Value::record(fields))
    }
}
