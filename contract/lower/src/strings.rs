//! @ref LLP 1060 D2–D4 — `t(…)` reads the locale slot; the tables it names
//! are baked after every body.

use super::*;
use crate::expr::compile;
use exact_plan::Stdlib;

impl Lowerer<'_> {
    /// `t("key", name=value, …)`, typed already: the locale slot, the key,
    /// and the placeholders as name/value string pairs, into the roster's
    /// `t`. The first call makes the slot; data-only tables also receive it.
    pub(crate) fn text_call(
        &mut self,
        asm: &mut Asm,
        args: &[Expr],
        span: Span,
        scope: &Scope,
        locals: &mut u16,
    ) -> Result<Ty, LowerError> {
        let (Some(Expr::Str(key, _)), Some(_)) = (args.first(), &self.types.shapes.strings) else {
            return err(
                "lower-strings",
                "`t` needs its key and the app's tables",
                span,
            );
        };
        let slot = self.locale_slot()?;
        self.texts_used.insert(key.clone());
        asm.load_slot(slot);
        asm.str(self.b.str(key));
        for arg in &args[1..] {
            let Expr::NamedArg(name, value, _) = arg else {
                return err(
                    "lower-strings",
                    "a placeholder is filled by name",
                    arg.span(),
                );
            };
            asm.str(self.b.str(name));
            if !compile(self, asm, value, scope, locals)?.is_string() {
                asm.call(Stdlib::ToString);
            }
        }
        asm.list(2 * (args.len() as u32 - 1));
        asm.call(Stdlib::T);
        Ok(Ty::String)
    }

    fn locale_slot(&mut self) -> Result<exact_plan::SlotsId, LowerError> {
        if let Some(slot) = self.locale {
            return Ok(slot);
        }
        let ty = self.ty_id(&Ty::String)?;
        let base = &self.types.shapes.strings.as_ref().expect("strings").base;
        let init = self.b.constant(&Value::str(base));
        let slot = self.b.slot("#locale", ty, init);
        self.locale = Some(slot);
        Ok(slot)
    }

    /// The tables' names, base first and the rest in name order.
    fn table_names(&self) -> Vec<String> {
        let Some(strings) = &self.types.shapes.strings else {
            return Vec::new();
        };
        let others = strings.tables.keys().filter(|l| **l != strings.base);
        std::iter::once(&strings.base)
            .chain(others)
            .cloned()
            .collect()
    }

    /// The format locale a localized formatter's call reads as its last
    /// argument (LLP 1054.000.003 D6): the slot, made by the first call,
    /// when some table's locale formats otherwise than `en-US`; else `""`.
    pub(crate) fn push_format_locale(&mut self, asm: &mut Asm) -> Result<(), LowerError> {
        if self.format.is_none() {
            let names = self.table_names();
            self.format = Some(None);
            if crate::formats::records(names.iter().map(String::as_str)).is_some() {
                let ty = self.ty_id(&Ty::String)?;
                let init = self.b.constant(&Value::str(&names[0]));
                self.format = Some(Some(self.b.slot("#format", ty, init)));
            }
        }
        match self.format.flatten() {
            Some(slot) => {
                asm.load_slot(slot);
            }
            None => {
                let empty = self.b.str("");
                asm.str(empty);
            }
        }
        Ok(())
    }

    /// The tables, base first and the rest in name order, each holding only
    /// the keys some `t` names, and each its format data when a formatter
    /// reads the format slot. Names and direction remain for data-only tables.
    pub(super) fn bake_texts(&mut self) -> Result<(), LowerError> {
        let Some(strings) = self.types.shapes.strings.clone() else {
            return Ok(());
        };
        let slot = self.locale_slot()?;
        let names = self.table_names();
        let records = match self.format {
            Some(Some(_)) => crate::formats::records(names.iter().map(String::as_str)),
            _ => None,
        };
        let direction = icu_locale::LocaleDirectionality::new_extended();
        for (i, locale) in names.iter().enumerate() {
            let table = &strings.tables[locale];
            let texts: Vec<(&str, &str)> = self
                .texts_used
                .iter()
                .filter_map(|key| Some((key.as_str(), table.get(key)?.as_str())))
                .collect();
            let rtl = locale
                .parse::<icu_locale::Locale>()
                .ok()
                .is_some_and(|locale| direction.is_right_to_left(&locale.id));
            let format = records.as_ref().map_or("", |r| r[i].as_str());
            self.b.locale(locale, rtl, format, &texts);
        }
        self.b.set_locale(slot);
        if let Some(Some(format)) = self.format {
            self.b.set_format(format);
        }
        Ok(())
    }
}
