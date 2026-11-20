//! DOM mount functions preserve the lexical bindings of their region or parent node.
use super::Em;
use crate::code::{self, Scope};
use std::fmt::Write as _;

impl Em<'_> {
    pub(super) fn mount_node(
        &mut self,
        node: u32,
        parent: &str,
        scope: &Scope,
    ) -> Result<(), String> {
        if !self
            .repeated_nodes
            .get(node as usize)
            .is_some_and(|shape| shape.repeated || shape.slot.is_some())
        {
            return self.node(node, parent, scope).map(|_| ());
        }
        let saved = std::mem::take(&mut self.out);
        let outer = self.next_local_element.replace(0);
        let scope = Scope {
            capture: true,
            ..scope.clone()
        };
        let slot = self.node(node, "p", &scope)?;
        self.next_local_element = outer;
        let body = std::mem::replace(&mut self.out, saved);
        // Keeping both calls in the enclosing template lets repeated arms share
        // their captures and relocate their diagnostic sites together.
        let body = if let Some(slot) = slot {
            let frame = self.mount_templates.slot_frame(body);
            format!("{frame}{slot});")
        } else {
            body
        };
        let mount = self.mount_templates.mount("p", body, Some(parent), None);
        self.out.push_str(&mount);
        Ok(())
    }

    pub(super) fn mount_children(
        &mut self,
        node: u32,
        parent: &str,
        scope: &Scope,
        same_scope: bool,
    ) -> Result<Option<String>, String> {
        let children = self.sites.of_node(node);
        let slot = self
            .repeated_nodes
            .get(node as usize)
            .and_then(|shape| shape.slot.clone());
        let Some(slot) = slot.filter(|_| same_scope && scope.capture) else {
            self.children(children, parent, scope)?;
            return Ok(None);
        };
        self.children(&children[..slot.start], parent, scope)?;
        let saved = std::mem::take(&mut self.out);
        let outer = self.next_local_element.replace(0);
        self.children(&children[slot.clone()], "p", scope)?;
        self.next_local_element = outer;
        let body = std::mem::replace(&mut self.out, saved);
        let callback = self.mount_templates.mount("p", body, None, None);
        write!(self.out, "$slot({parent});").unwrap();
        if slot.end < children.len() {
            let tail = Scope {
                capture_sites: true,
                ..scope.clone()
            };
            self.children(&children[slot.end..], parent, &tail)?;
        }
        Ok(Some(callback))
    }

    pub(super) fn site_number(&self, scope: &Scope, n: u32) -> String {
        if scope.capture_sites {
            scope.operand(n.to_string())
        } else if self.next_local_element.is_some() {
            format!("\0s{n}\0")
        } else {
            n.to_string()
        }
    }

    pub(super) fn site_evaluation(
        &self,
        scope: &Scope,
        n: u32,
        what: String,
        instance: bool,
    ) -> Scope {
        if self.next_local_element.is_none() {
            return code::evaluation(scope, format!("site {n} {what}"), instance);
        }
        let suffix = serde_json::to_string(&format!(" {what}")).unwrap();
        let label = format!(r#""site "+({})+{suffix}"#, self.site_number(scope, n));
        Scope {
            evaluation: Some((label, instance)),
            ..scope.clone()
        }
    }

    /// Descendant arms may close over bindings introduced here, including a
    /// keyed row's getters or the DOM element that owns a navigation status.
    pub(super) fn template_scope(
        &mut self,
        new_scope: bool,
        render: impl FnOnce(&mut Self) -> Result<(), String>,
    ) -> Result<(), String> {
        if !new_scope {
            return render(self);
        }
        let outer = std::mem::take(&mut self.mount_templates);
        self.mount_templates.depth = outer.depth + 1;
        let saved = std::mem::take(&mut self.out);
        render(self)?;
        let built = std::mem::replace(&mut self.out, saved);
        let templates = std::mem::replace(&mut self.mount_templates, outer);
        let (declarations, view) = templates.render(built, self.next_local_element.is_some(), "$v");
        if declarations.is_empty() {
            self.out.push_str(&view);
        } else {
            write!(self.out, "{{{declarations}{view}}}").unwrap();
        }
        Ok(())
    }
}
