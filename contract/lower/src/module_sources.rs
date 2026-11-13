//! The sources of each used module, including packages used through a view alone.

use crate::{LowerError, Lowerer};
use contract_syntax::{File, Node};
use contract_types::{ComponentTypes, Ty};

impl Lowerer<'_> {
    pub(crate) fn declare_module_sources(
        &mut self,
        file: &File,
        root_types: &ComponentTypes,
        view: &[Node],
    ) -> Result<(), Vec<LowerError>> {
        // The seam's signatures (LLP 1027 D2), in name order; a native module's
        // names among them are routed to the native slot (LLP 1133.002 D13).
        // A used module's other names are there too, unread: its substitute reads
        // them (its status, the source a send answers from).
        let mut sources = root_types.sources.clone();
        // An event the root hears uses its module too (LLP 1133.004 D6), and so
        // does a name data code alone calls (LLP 1133.011 D12), which Contract
        // never reads: a package of such names alone (`@exact/clipboard`, LLP
        // 1133.006) is used once an explicit `use` reads its declaration. So does a
        // package the app's TypeScript imports or its expanded view places.
        let views = crate::native_typed::used_extensions(view);
        let used: Vec<&str> = (file.modules.iter())
            .filter(|m| {
                sources.contains_key(&m.name)
                    || m.handler.is_some()
                    || (m.data_only && !m.view_only)
                    || m.imported
                    || views.contains(&m.extension)
            })
            .map(|m| m.extension.as_str())
            .collect();
        // An `img` scheme is one package's (LLP 1133.019 D11 item 1).
        let mut schemes: std::collections::BTreeMap<String, &str> = Default::default();
        for m in (file.modules.iter()).filter(|m| used.contains(&m.extension.as_str())) {
            let sub: serde_json::Value = serde_json::from_str(&m.substitute).unwrap_or_default();
            let Some(scheme) = sub["images"]["scheme"].as_str() else {
                continue;
            };
            match schemes.insert(scheme.to_owned(), &m.package) {
                Some(other) if other != m.package => {
                    return Err(vec![LowerError {
                        id: "bake-scheme-twice",
                        message: format!(
                            "the image scheme `{scheme}:` is declared by {other} and by {}: a scheme is one package's, so the app uses one of them",
                            m.package
                        ),
                        span: m.span,
                    }])
                }
                _ => {}
            }
        }
        for m in file.modules.iter().filter(|m| {
            used.contains(&m.extension.as_str()) && m.kind != contract_syntax::ModuleKind::Event
        }) {
            let options = format!("{}__options", m.name);
            let params = match self.types.shapes.map.contains_key(&options) {
                true => vec![Ty::Record(options)],
                false => Vec::new(),
            };
            // A send that answers nothing answers `unit` (LLP 1133.004 D5).
            let answers = match &m.answers {
                Some(a) => self.types.shapes.resolve(a),
                None => Ok(Ty::Unit),
            };
            if let Ok(ty) = answers {
                sources.entry(m.name.clone()).or_insert((params, ty));
            }
        }
        let fetches = self.declared_sources(file, &mut sources)?;
        for (name, (params, ty)) in &sources {
            let mut ids = Vec::with_capacity(params.len());
            for p in params {
                ids.push(self.ty_id(p)?);
            }
            let ty = self.ty_id(ty)?;
            let id = self.b.source(name, &ids, ty);
            if let Some(f) = fetches.get(name) {
                let params: Vec<&str> = f.params.iter().map(String::as_str).collect();
                let headers: Vec<(&str, &str)> = (f.headers.iter())
                    .map(|(n, v)| (n.as_str(), v.as_str()))
                    .collect();
                self.b.fetch(
                    id,
                    &f.method,
                    &f.url,
                    &f.origin,
                    &params,
                    &headers,
                    f.body,
                    &f.document,
                    &f.pointer,
                );
            }
            if let Some(m) = file.modules.iter().find(|m| m.name == *name) {
                self.b.module(
                    id,
                    (&m.extension, &m.package),
                    &m.declared,
                    m.kind == contract_syntax::ModuleKind::Send,
                    m.live,
                    &m.request,
                    m.sync.as_deref().unwrap_or(""),
                    &m.substitute,
                );
            }
        }
        Ok(())
    }
}
