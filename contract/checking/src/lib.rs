//! Checking views retain the exact typed expansion and authored provenance.
//! Structural regions expose only checking visits through the shared traversal.
//! @ref LLP 1172.019 §2 item 4; LLP 1172.013 E1, §4 step 4.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

use contract_syntax::{Attr, Expr, Node, Span};
use contract_types::{Checked, Scope};
use std::collections::BTreeMap;
use std::sync::Arc;

mod values;
pub use values::value_leaves;
#[cfg(test)]
mod census;
#[cfg(test)]
mod tests;

/// A borrowed checking representation: type obligations and expansion stay
/// owned by `Checked`, rather than reconstructed from a lowered syntax tree.
pub struct Checking<'a> {
    checked: &'a Checked<'a>,
    declarations: BTreeMap<&'a str, usize>,
}

impl<'a> Checking<'a> {
    /// Retain the type checker's output without cloning or re-expanding it.
    pub fn new(checked: &'a Checked<'a>) -> Self {
        Self {
            checked,
            declarations: checked
                .file
                .components
                .iter()
                .enumerate()
                .map(|(i, c)| (c.name.as_str(), i))
                .collect(),
        }
    }

    /// Declaration facts in source order, without a component's view tree.
    pub fn declarations(&self) -> impl ExactSizeIterator<Item = Declaration<'a>> {
        self.checked
            .file
            .components
            .iter()
            .zip(&self.checked.types.components)
            .map(|(component, types)| Declaration { component, types })
    }

    /// A declaration identified by a component invocation.
    pub fn declaration(&self, index: usize) -> Declaration<'a> {
        Declaration {
            component: &self.checked.file.components[index],
            types: &self.checked.types.components[index],
        }
    }

    /// Declared sound paths, in source order.
    pub fn sounds(&self) -> impl Iterator<Item = &'a str> {
        self.checked
            .file
            .sounds
            .iter()
            .map(|sound| sound.source.as_str())
    }

    /// Expanded root states, derives and providers, in that order.
    pub fn root_bindings(&self) -> impl Iterator<Item = &'a contract_syntax::Binding> {
        let root = &self.checked.expanded.root;
        root.states
            .iter()
            .chain(&root.derives)
            .chain(&root.provides)
    }

    /// Expanded root state initializers, in declaration order.
    pub fn states(&self) -> &'a [contract_syntax::Binding] {
        &self.checked.expanded.root.states
    }

    /// Expanded root derives, in declaration order.
    pub fn derives(&self) -> &'a [contract_syntax::Binding] {
        &self.checked.expanded.root.derives
    }

    /// Inferred root action parameter types, in declaration order.
    pub fn action_types(&self) -> &'a [Vec<contract_types::Ty>] {
        &self.checked.types.components[0].actions
    }

    /// Expanded root resource declarations, which contain no view tree.
    pub fn resources(&self) -> &'a [contract_syntax::ResourceDecl] {
        &self.checked.expanded.root.resources
    }

    /// Expanded root sequence declarations, which contain no view tree.
    pub fn sequences(&self) -> &'a [contract_syntax::Sequence] {
        &self.checked.expanded.root.sequences
    }

    /// Function and record types shared with expression inference.
    pub fn shapes(&self) -> &'a contract_types::Shapes {
        &self.checked.types.shapes
    }

    /// Authored record names, excluding generated routing shapes.
    pub fn declared_shapes(&self) -> impl Iterator<Item = &'a str> {
        self.checked
            .file
            .shapes
            .iter()
            .map(|shape| shape.name.as_str())
    }

    /// Regions that own lifted component state.
    pub fn state_regions(&self) -> impl Iterator<Item = u32> + '_ {
        self.checked
            .expanded
            .owners
            .iter()
            .filter_map(|owner| match owner {
                contract_syntax::Owner::Arm { tag, .. } => Some(*tag),
                _ => None,
            })
    }

    /// Function declarations, which contain expressions but no view tree.
    pub fn functions(&self) -> &'a [contract_syntax::FnDecl] {
        &self.checked.file.fns
    }

    /// Authored class declarations in their precedence order.
    pub fn styles(&self) -> &'a [contract_syntax::StyleDecl] {
        &self.checked.file.styles
    }

    /// Root actions include the lifted declarations checked by inference.
    pub fn actions(&self, component: usize) -> &'a [contract_syntax::Action] {
        if component == 0 {
            &self.checked.expanded.root.actions
        } else {
            &self.checked.file.components[component].actions
        }
    }

    /// An unknown invocation's diagnostic still uses the authored vocabulary.
    pub fn unknown_component_message(&self, name: &str) -> String {
        self.checked.file.unknown_component_message(name)
    }

    /// The authored view with its checked declaration scope. The root uses
    /// lifted declarations, as analysis did before this representation.
    pub fn authored(&'a self, component: usize) -> View<'a> {
        let c = &self.checked.file.components[component];
        let scoped = if component == 0 {
            &self.checked.expanded.root
        } else {
            c
        };
        View {
            nodes: &c.view,
            declarations: &self.declarations,
            scope: self
                .checked
                .types
                .component_scope(scoped, &self.checked.types.components[component]),
            shapes: &self.checked.types.shapes,
        }
    }

    /// The expanded root, keeping the row ownership used during inference.
    pub fn expanded(&'a self) -> View<'a> {
        let c = &self.checked.expanded.root;
        View {
            nodes: &c.view,
            declarations: &self.declarations,
            scope: self
                .checked
                .types
                .component_scope(c, &self.checked.types.components[0]),
            shapes: &self.checked.types.shapes,
        }
    }
}

/// A checked view; its tree can only be read through traversal.
pub struct View<'a> {
    nodes: &'a [Node],
    declarations: &'a BTreeMap<&'a str, usize>,
    scope: Scope,
    shapes: &'a contract_types::Shapes,
}

impl<'a> View<'a> {
    /// Declaration scope for expressions outside the view's regions.
    pub fn scope(&self) -> &Scope {
        &self.scope
    }

    /// Structural facts without assigning a new lexical scope.
    pub fn structure(&self) -> Structure<'a> {
        Structure {
            nodes: self.nodes,
            declarations: self.declarations,
        }
    }

    /// Every node, in source order, without inferring region subjects.
    /// Structural checks must also see arms whose subject could not be typed.
    pub fn walk(&self) -> impl Iterator<Item = Visit<'a>> {
        self.walk_with((), |_, _| ()).map(|(visit, ())| visit)
    }

    /// One inherited fact per subtree, with sibling and arm isolation.
    /// O(nodes) descent, O(depth) heap frames, no recursive Rust calls.
    /// `descend` runs before yielding the parent and selects its children's fact.
    pub fn walk_with<C: Clone>(
        &self,
        context: C,
        descend: impl FnMut(Visit<'a>, &C) -> C,
    ) -> impl Iterator<Item = (Visit<'a>, C)> {
        self.structure().walk_with(context, descend)
    }

    /// Preorder with roots, siblings and arms in reverse source order.
    pub fn walk_reverse(&self) -> impl Iterator<Item = Visit<'a>> {
        self.structure().walk_reverse()
    }

    /// Enter and leave each node, with inherited facts and optional pruning.
    /// The boolean is true on entry. Leave retains the incoming facts.
    pub fn events<C: Clone>(
        &self,
        context: C,
        descend: impl FnMut(Visit<'a>, &C) -> Option<C>,
    ) -> impl Iterator<Item = (Visit<'a>, C, bool)> {
        self.structure().events(context, descend)
    }

    /// Scoped descent with one fact per arm; inference stays in the shared
    /// region primitive. The callback sees the incoming scope and arm binders.
    pub fn scoped_with<C: Clone>(
        &self,
        context: C,
        mut descend: impl FnMut(Visit<'a>, &Scope, &C, contract_syntax::Binds<'a>) -> C,
    ) -> impl Iterator<Item = (Visit<'a>, Arc<Scope>, C)> {
        let declarations = self.declarations;
        let shapes = self.shapes;
        Node::walk_with(
            self.nodes,
            (Arc::new(self.scope.clone()), context),
            move |node, (scope, context)| {
                let visit = Visit { node, declarations };
                let frames = contract_types::view_frames(node, scope, shapes);
                let lists = node.scoped_child_lists();
                let mut i = 0;
                frames.map(|frame| {
                    let binds = lists[i].1;
                    i += 1;
                    frame.map(|frame| (frame, descend(visit, scope, context, binds)))
                })
            },
        )
        .map(|(node, (scope, context))| (Visit { node, declarations }, scope, context))
    }

    /// Names resolved under the plan's region frames. Subjects are inferred
    /// once per region; siblings share immutable scope frames. Work includes
    /// subject inference, and storage is O(view depth × scope depth).
    pub fn scoped(&self) -> impl Iterator<Item = (Visit<'a>, Arc<Scope>)> {
        let shapes = self.shapes;
        let declarations = self.declarations;
        Node::walk_with(
            self.nodes,
            Arc::new(self.scope.clone()),
            move |node, scope| contract_types::view_frames(node, scope, shapes),
        )
        .map(|(node, scope)| (Visit { node, declarations }, scope))
    }
}

/// A structural region with private nodes. It carries no invented lexical scope;
/// expression inference uses `View::scoped` or `View::scoped_with`.
#[derive(Clone, Copy)]
pub struct Structure<'a> {
    nodes: &'a [Node],
    declarations: &'a BTreeMap<&'a str, usize>,
}

impl<'a> Structure<'a> {
    /// Direct members in source order, without entering their subtrees.
    pub fn roots(self) -> impl DoubleEndedIterator<Item = Visit<'a>> {
        self.nodes.iter().map(move |node| Visit {
            node,
            declarations: self.declarations,
        })
    }

    /// Every node, in source order, without inferring region subjects.
    /// Structural checks must also see arms whose subject could not be typed.
    pub fn walk(self) -> impl Iterator<Item = Visit<'a>> {
        self.walk_with((), |_, _| ()).map(|(visit, ())| visit)
    }

    /// One inherited fact per subtree, with sibling and arm isolation.
    /// O(nodes) descent, O(depth) heap frames, no recursive Rust calls.
    /// `descend` runs before yielding the parent and selects its children's fact.
    pub fn walk_with<C: Clone>(
        self,
        context: C,
        mut descend: impl FnMut(Visit<'a>, &C) -> C,
    ) -> impl Iterator<Item = (Visit<'a>, C)> {
        self.walk_regions_with(context, move |node, context| {
            let child = descend(node, context);
            [Some(child.clone()), Some(child)]
        })
    }

    /// Inherited facts and pruning selected separately for each direct region.
    /// Siblings retain their region's context; choices can isolate mutable facts.
    pub fn walk_regions_with<C: Clone>(
        self,
        context: C,
        mut descend: impl FnMut(Visit<'a>, &C) -> [Option<C>; 2],
    ) -> impl Iterator<Item = (Visit<'a>, C)> {
        let declarations = self.declarations;
        Node::walk_with(self.nodes, context, move |node, context| {
            descend(Visit { node, declarations }, context)
        })
        .map(|(node, context)| (Visit { node, declarations }, context))
    }

    /// Preorder with roots, siblings and arms in reverse source order.
    pub fn walk_reverse(self) -> impl Iterator<Item = Visit<'a>> {
        Node::walk_reverse(self.nodes).map(|node| Visit {
            node,
            declarations: self.declarations,
        })
    }

    /// Enter and leave each node, with inherited facts and optional pruning.
    /// The boolean is true on entry. Leave retains the incoming facts.
    pub fn events<C: Clone>(
        self,
        context: C,
        mut descend: impl FnMut(Visit<'a>, &C) -> Option<C>,
    ) -> impl Iterator<Item = (Visit<'a>, C, bool)> {
        let declarations = self.declarations;
        Node::walk_events_with(self.nodes, context, move |node, context| {
            let child = descend(Visit { node, declarations }, context);
            [child.clone(), child]
        })
        .map(|(node, context, entering)| (Visit { node, declarations }, context, entering))
    }
}

/// Node identity within one borrowed checking expansion, including copies
/// with the same authored span and component instance.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Site(usize);

/// A single node's facts, with structural queries over its private regions.
#[derive(Clone, Copy)]
pub struct Visit<'a> {
    node: &'a Node,
    declarations: &'a BTreeMap<&'a str, usize>,
}

impl<'a> Visit<'a> {
    /// Identity for facts computed by a traversal of this expansion.
    pub fn site(self) -> Site {
        Site(self.node as *const Node as usize)
    }

    /// This node and its descendants, retaining expansion identity.
    pub fn subtree(self) -> Structure<'a> {
        Structure {
            nodes: std::slice::from_ref(self.node),
            declarations: self.declarations,
        }
    }

    /// Direct regions in source order. A choice has two arms, a repetition
    /// one body and an empty second region; elements and uses hold children.
    pub fn regions(self) -> [Structure<'a>; 2] {
        self.node.child_lists().map(|nodes| Structure {
            nodes,
            declarations: self.declarations,
        })
    }

    /// Direct members of all regions in source order.
    pub fn children(self) -> impl DoubleEndedIterator<Item = Visit<'a>> {
        self.regions().into_iter().flat_map(Structure::roots)
    }

    /// Whether this node chooses between two conditional regions.
    pub fn is_choice(self) -> bool {
        matches!(self.node, Node::When { .. } | Node::Match { .. })
    }

    /// A view `if`'s condition.
    pub fn condition(self) -> Option<&'a Expr> {
        match self.node {
            Node::When { cond, .. } => Some(cond),
            _ => None,
        }
    }

    /// Names bound over each direct region, in the same order as `regions`.
    pub fn bindings(self) -> [contract_syntax::Binds<'a>; 2] {
        self.node.scoped_child_lists().map(|(_, binds)| binds)
    }

    /// Authored source identity and token range.
    pub fn span(self) -> Span {
        self.node.span()
    }

    /// An element's tag, positional values and attributes. Child lists are private.
    pub fn element(self) -> Option<(&'a str, &'a [Expr], &'a [Attr])> {
        match self.node {
            Node::Element {
                tag,
                positional,
                attrs,
                ..
            } => Some((tag, positional, attrs)),
            _ => None,
        }
    }

    /// Expansion identity used by per-instance lowering facts.
    pub fn instance(self) -> Option<u32> {
        match self.node {
            Node::Element { instance, .. } => Some(*instance),
            _ => None,
        }
    }

    /// A repetition's region identity, binders, subject and key.
    pub fn repetition(self) -> Option<(u32, &'a str, Option<&'a str>, &'a Expr, &'a Expr)> {
        match self.node {
            Node::Each {
                tag,
                var,
                index,
                list,
                key,
                ..
            } => Some((*tag, var, index.as_deref(), list, key)),
            _ => None,
        }
    }

    /// A conditional or repetition's state-owner region identity.
    pub fn region(self) -> Option<u32> {
        match self.node {
            Node::When { tag, .. } | Node::Each { tag, .. } | Node::Match { tag, .. } => Some(*tag),
            _ => None,
        }
    }

    /// Whether this is an unresolved children slot.
    pub fn is_slot(self) -> bool {
        matches!(self.node, Node::Children { .. })
    }

    /// Expressions held by this node, before entering its regions.
    pub fn expressions(self) -> impl Iterator<Item = &'a Expr> {
        self.node.exprs()
    }

    /// Structural descent through a repetition's body, empty for other nodes.
    /// Scoped expression checks use `View::scoped_with` instead.
    pub fn repetition_body(self) -> impl Iterator<Item = Visit<'a>> {
        let nodes = match self.node {
            Node::Each { body, .. } => body.as_slice(),
            _ => &[],
        };
        Node::walk(nodes).map(move |node| Visit {
            node,
            declarations: self.declarations,
        })
    }

    /// A component invocation's declaration identity, authored name and arguments.
    /// Unknown declarations retain their name for admission diagnostics.
    pub fn component_use(self) -> Option<(Option<usize>, &'a str, &'a [Attr])> {
        match self.node {
            Node::Use { name, args, .. } => {
                Some((self.declarations.get(name.as_str()).copied(), name, args))
            }
            _ => None,
        }
    }
}

/// Component declaration facts, with the view kept private.
#[derive(Clone, Copy)]
pub struct Declaration<'a> {
    component: &'a contract_syntax::Component,
    types: &'a contract_types::ComponentTypes,
}

impl<'a> Declaration<'a> {
    /// Authored component name.
    pub fn name(self) -> &'a str {
        &self.component.name
    }
    /// Explicit parameters in declaration order.
    pub fn props(self) -> &'a [contract_syntax::Param] {
        &self.component.props
    }
    /// Injected parameters, after explicit parameters in the typed scope.
    pub fn injects(self) -> &'a [contract_syntax::Param] {
        &self.component.injects
    }
    /// Provider expressions evaluated in this declaration's scope.
    pub fn provides(self) -> &'a [contract_syntax::Binding] {
        &self.component.provides
    }
    /// Inferred explicit and injected parameter types, in scope order.
    pub fn prop_types(self) -> &'a [contract_types::Ty] {
        &self.types.props
    }
}
