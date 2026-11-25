//! Conservative initial flow shape for opt-in variable-size collections: a
//! block list scrolls vertically; a flex row scrolls horizontally, CSS's
//! carousel (LLP 1070 H2).
use super::{err, values::numeric_literal, LowerError, Lowerer};
use contract_syntax::{Attr, Expr, Node, Span};
use contract_types::{Scope, Ty};

fn literal<'a>(attrs: &'a [Attr], name: &str) -> Option<&'a Attr> {
    attrs.iter().find(|a| a.name == name)
}
fn string(a: &Attr) -> Option<&str> {
    match &a.value {
        Expr::Str(s, _) => Some(s),
        _ => None,
    }
}

/// Whether a virtualized list's attributes make it a row list: `display:
/// flex`, whose `flex-direction` is CSS's default `row`. Only literals
/// count; the collection's axis is fixed when it is created.
pub(super) fn row_list(attrs: &[Attr]) -> bool {
    literal(attrs, "display").and_then(string) == Some("flex")
}

/// `reach-margin` (@ref LLP 1145 D4): one or two literal nonnegative px or
/// % lengths, before and after, as the runner and list.js read them.
fn reach_margin(a: &Attr, virtualized: bool) -> Result<(), LowerError> {
    if !virtualized {
        return err("lower-list-virtualized", "`reach-margin` sets how far ahead of an edge a virtualized list's `reachstart` and `reachend` fire; it goes on `list virtualized=true`", a.span);
    }
    let length = |part: &str| {
        let part = part.to_ascii_lowercase();
        part.strip_suffix("px")
            .or_else(|| part.strip_suffix('%'))
            .is_some_and(|n| {
                super::intersect::css_number(n)
                    && n.parse::<f64>().is_ok_and(|n| n.is_finite() && n >= 0.0)
            })
    };
    match string(a) {
        Some(s)
            if (1..=2).contains(&s.split_ascii_whitespace().count())
                && s.split_ascii_whitespace().all(length) =>
        {
            Ok(())
        }
        _ => err("lower-reach-margin", "`reach-margin` takes a literal: one or two nonnegative px or % lengths, before and after, a % being of the list's port (for example \"200%\" or \"0px 800px\"); the default is one port, \"100%\"", a.span),
    }
}

fn fixed_offset(value: &Expr) -> bool {
    numeric_literal(value) == Some(0.0)
        || matches!(value, Expr::Str(s, _) if s == "auto"
            || s.strip_suffix("px").unwrap_or(s).parse::<f64>() == Ok(0.0))
}

fn fixed_margin(value: &Expr) -> bool {
    let number = match value {
        Expr::Str(s, _) => exact_kernel::style::cells::parse(s)
            .or_else(|| exact_kernel::style::relative::parse(s).map(|(_, n)| n))
            .or_else(|| {
                let mut style = exact_kernel::StyleProps::default();
                style
                    .set_dynamic(
                        exact_kernel::StyleId::MarginTop,
                        &exact_kernel::StyleValue::Text(s.into()),
                    )
                    .ok()?;
                match style.margin_top {
                    exact_kernel::Dimension::Points(n) | exact_kernel::Dimension::Percent(n) => {
                        Some(n)
                    }
                    _ => None,
                }
            })
            .map(f64::from),
        _ => numeric_literal(value),
    };
    number.is_some_and(|n| n.is_finite() && n >= 0.0)
}

impl Lowerer<'_> {
    /// `keep-mounted=keys` (@ref LLP 1010 §6.11): a list of row keys, each a
    /// string, number or bool or an option of one, as a `for`'s `key=` is.
    pub(super) fn bind_keep_mounted(
        &mut self,
        attr: &Attr,
        scope: &Scope,
        locals: u16,
        bindings: &mut Vec<exact_plan::BindingsRow>,
    ) -> Result<(), LowerError> {
        let (code, ty) = self.typed_code(&attr.value, scope, locals)?;
        let key = |t: &Ty| {
            matches!(
                t,
                Ty::String | Ty::Number | Ty::Bool | Ty::Enum(_) | Ty::Unknown
            )
        };
        let keys = match &ty {
            Ty::List(item) => match item.as_ref() {
                Ty::Option(inner) => key(inner),
                item => key(item),
            },
            _ => false,
        };
        if !keys {
            return err(
                "lower-attr-type",
                format!("`keep-mounted` takes a list of row keys, each a string, number or bool (or an option of one, `none` naming no row), as the `for`'s `key=` gives them: `keep-mounted=[first(ids), ids.at(-1)]`; this is {ty}"),
                attr.span,
            );
        }
        bindings.push(super::prop_row(
            exact_kernel::PropId::KeepMounted,
            code,
            attr.written,
        ));
        Ok(())
    }

    pub(super) fn check_collection(
        &self,
        tag: &str,
        attrs: &[Attr],
        children: &[Node],
        span: Span,
        scope: &Scope,
    ) -> Result<(), LowerError> {
        let virtualized = tag == "list"
            && attrs
                .iter()
                .any(|a| a.name == "virtualized" && matches!(a.value, Expr::Bool(true, _)));
        if !virtualized {
            if let Some(edge) = attrs
                .iter()
                .find(|a| matches!(a.name.as_str(), "reachstart" | "reachend"))
            {
                return err(
                    "lower-collection-edge",
                    super::vocab::collection_edge_refusal(&edge.name),
                    edge.span,
                );
            }
        }
        if let Some(margin) = literal(attrs, "reach-margin") {
            reach_margin(margin, virtualized)?;
        }
        if let Some(keep) = literal(attrs, "keep-mounted") {
            if !virtualized {
                return err("lower-list-virtualized", "`keep-mounted` names rows a virtualized list keeps mounted wherever its window is; it goes on `list virtualized=true`, whose rows are the only ones that unmount", keep.span);
            }
            self.check_kept_keys(keep, children, scope)?;
        }
        if let Some(reorder) = attrs.iter().find(|a| a.name == "reorderdrop") {
            if !virtualized && !(tag == "list" && literal(attrs, "listStyle").is_some()) {
                return err("lower-reorder-collection", format!("`reorderdrop` needs a vertical virtualized list or a grouped settings list. On `{tag}`, write `list virtualized=true` with one keyed `for`, or `list appearance=\"auto\"` with sections; name the list's id with `reorderFor`"), reorder.span);
            }
            let layout = self.with_class_rows(attrs);
            let display = literal(&layout, "display");
            let horizontal = matches!(display.and_then(string), Some("flex" | "inline-flex"))
                && literal(&layout, "flex-direction").and_then(string) != Some("column");
            if !virtualized
                && (display
                    .is_some_and(|a| matches!(string(a), None | Some("grid" | "inline-grid")))
                    || horizontal)
            {
                return err(
                    "lower-collection-reorder",
                    "reordering a grouped list needs vertical sections in block flow",
                    reorder.span,
                );
            }
        }
        if tag == "list" {
            for child in Node::walk(children) {
                if let Node::Element {
                    tag, attrs: inner, ..
                } = child
                {
                    if tag == "list"
                        && (literal(attrs, "listStyle").is_some()
                            && literal(attrs, "reorderdrop").is_some()
                            || literal(inner, "reorderdrop").is_some()
                                && crate::grouped::style(tag, &self.with_class_rows(inner))?
                                    .is_some())
                    {
                        return err("lower-collection-reorder", "reordering a grouped list is confined to a single list; move the nested list outside it", child.span());
                    }
                }
            }
        }
        self.check_group(virtualized, attrs, children, scope)?;
        let Some(opt) = attrs.iter().find(|a| a.name == "virtualized") else {
            return Ok(());
        };
        if tag != "list" || !matches!(opt.value, Expr::Bool(..)) {
            return err(
                "lower-collection-opt-in",
                "`virtualized` requires a literal boolean on `list`",
                opt.span,
            );
        }
        if matches!(opt.value, Expr::Bool(false, _)) {
            return Ok(());
        }
        // A class's rows are its literals: a percentage there is refused too.
        let own = self.with_class_rows(attrs);
        let row = row_list(&own);
        self.collection_axis(attrs, row, span, scope)?;
        // Computed bounds are checked by the bake's measured-layout lint.
        // Merely spelling `auto` or a non-growing flex is not a bound.
        if !row
            && !attrs.iter().any(|a| match a.name.as_str() {
                "height" | "max-height" => !matches!(&a.value, Expr::Str(s, _) if s == "auto"),
                "flex" => super::values::flex_bounds(&a.value),
                _ => false,
            })
        {
            return err("lower-collection-unbounded", "a virtualized list needs height, max-height, or flex constraining its vertical scrollport", span);
        }
        self.collection_inset(&own, row, scope)?;
        self.collection_flow(attrs, Some(row))?;
        let mut rows = children.iter().filter(|n| matches!(n, Node::Each { .. }));
        let Some(Node::Each { body, .. }) = rows.next() else {
            return err(
                "lower-collection-template",
                "a virtualized list needs one direct keyed `for`; put its data rows in that `for`",
                span,
            );
        };
        if rows.next().is_some() {
            return err("lower-collection-template", "a virtualized list holds one direct `for`; put another `for` inside an element (all its rows mount), or use a separate list", span);
        }
        if children.len() > 1 {
            if row {
                return err("lower-collection-fixed", "fixed content needs a vertical virtualized list; move it outside this horizontal list", span);
            }
            if literal(&own, "appearance").and_then(string) != Some("none") {
                return err("lower-collection-fixed", "fixed content in a virtualized cells list is not supported yet; write appearance=\"none\" on the list", span);
            }
            for fixed in children.iter().filter(|n| !matches!(n, Node::Each { .. })) {
                let fixed = std::slice::from_ref(fixed);
                // Its flow roots: elements other than `display: contents`, seen
                // through those and through `if`, `for` and `match`.
                let roots = Node::walk_with(fixed, (), |node, _| {
                    let through = match node {
                        Node::Element { attrs, .. } => {
                            literal(&self.with_class_rows(attrs), "display").and_then(string)
                                == Some("contents")
                        }
                        _ => true,
                    };
                    [through.then_some(()); 2]
                });
                for (node, ()) in roots {
                    if let Node::Element {
                        tag, attrs, span, ..
                    } = node
                    {
                        self.check_fixed_root(tag, attrs, *span)?;
                    }
                }
                for node in Node::walk(fixed) {
                    if let Node::Element { attrs, .. } = node {
                        if let Some(opt) = literal(attrs, "virtualized")
                            .filter(|a| !matches!(a.value, Expr::Bool(false, _)))
                        {
                            return err("lower-collection-fixed", "a virtualized list cannot live in fixed content; move it into a data row or outside the enclosing list, or write virtualized=false", opt.span);
                        }
                    }
                }
            }
        }
        self.check_nested(body, false)?;
        let [Node::Element { tag, attrs, .. }] = body.as_slice() else {
            return err("lower-collection-template", "a `for` in a virtualized list needs exactly one element flow root (wrap conditional or multiple roots in column)", span);
        };
        if tag == "dialog" {
            return err(
                "lower-collection-flow",
                "a virtual row root must participate in normal flow",
                span,
            );
        }
        self.collection_flow(&self.with_class_rows(attrs), None)
    }

    /// An element's attributes followed by its class's rows. An unknown
    /// class is the element's own refusal; here it has no rows.
    fn with_class_rows(&self, attrs: &[Attr]) -> Vec<Attr> {
        let mut expanded = attrs.to_vec();
        if let Some((_, rows)) = self.class_rows(attrs).ok().flatten() {
            expanded.extend(rows);
        }
        expanded
    }

    /// `reorderGroup` (@ref LLP 1094 D1): lists that share one exchange rows,
    /// so each takes the drop (`reorderdrop`), is named by its `id` in the
    /// drop's `ReorderEvent`, and keys its rows by strings, the keys a row
    /// carries from one list to another. A row list and a nested list are
    /// refused by their own reorder rules (LLP 1094 §7).
    fn check_group(
        &self,
        virtualized: bool,
        attrs: &[Attr],
        children: &[Node],
        scope: &Scope,
    ) -> Result<(), LowerError> {
        let Some(group) = attrs.iter().find(|a| a.name == "reorderGroup") else {
            if let Some(event) = attrs.iter().find(|a| a.name == "reordertargetchange") {
                return err(
                    "lower-reorder-group",
                    "`reordertargetchange` needs a list with a `reorderGroup`",
                    event.span,
                );
            }
            return Ok(());
        };
        let has = |name: &str| attrs.iter().any(|a| a.name == name);
        if !virtualized || !has("reorderdrop") || !has("id") {
            return err("lower-reorder-group", "`reorderGroup` joins lists that take a drop: give this `list virtualized=true` a `reorderdrop` and an `id`", group.span);
        }
        let Some(Node::Each {
            var,
            index,
            list,
            key,
            ..
        }) = children.iter().find(|n| matches!(n, Node::Each { .. }))
        else {
            return Ok(());
        };
        let shapes = &self.types.shapes;
        let item = match contract_types::infer(list, scope, shapes) {
            Ok(Ty::List(item)) => *item,
            _ => Ty::Unknown,
        };
        let mut inner = scope.clone();
        inner.push_each(var, index.as_deref(), item);
        match contract_types::infer(key, &inner, shapes) {
            Ok(t) if !t.is_string() && t.is_complete() => err("lower-reorder-group", format!("a grouped list's rows move between lists by their keys, so its `for` is keyed by a string, not a {t}: key it by the item's string id"), key.span()),
            _ => Ok(()),
        }
    }

    /// `keep-mounted`'s keys are the `for`'s key type: a number never names
    /// a row keyed by a string (@ref LLP 1010 §6.11).
    fn check_kept_keys(
        &self,
        keep: &Attr,
        children: &[Node],
        scope: &Scope,
    ) -> Result<(), LowerError> {
        let Some(Node::Each {
            var,
            index,
            list,
            key,
            ..
        }) = children.iter().find(|n| matches!(n, Node::Each { .. }))
        else {
            return Ok(());
        };
        let shapes = &self.types.shapes;
        let kind = |t: &Ty| match t {
            Ty::String | Ty::Enum(_) => Some("string"),
            Ty::Number => Some("number"),
            Ty::Bool => Some("bool"),
            _ => None,
        };
        let kept = match contract_types::infer(&keep.value, scope, shapes) {
            Ok(Ty::List(item)) => match *item {
                Ty::Option(inner) => kind(&inner),
                item => kind(&item),
            },
            _ => None,
        };
        let item = match contract_types::infer(list, scope, shapes) {
            Ok(Ty::List(item)) => *item,
            _ => Ty::Unknown,
        };
        let mut inner = scope.clone();
        inner.push_each(var, index.as_deref(), item);
        let keyed = contract_types::infer(key, &inner, shapes)
            .ok()
            .and_then(|t| kind(&t));
        match (kept, keyed) {
            (Some(kept), Some(keyed)) if kept != keyed => err("lower-attr-type", format!("`keep-mounted` names rows by the `for`'s key, a {keyed}; these keys are {kept}s and would name no row"), keep.span),
            _ => Ok(()),
        }
    }

    /// Whether `value` is a number of points: a literal one at least
    /// `least` (`positive`: more than it), or a computed one; a string could
    /// be `auto` or a percentage.
    fn points(&self, value: &Expr, scope: &Scope, positive: bool) -> bool {
        match (numeric_literal(value), value) {
            (Some(n), _) => n.is_finite() && if positive { n > 0.0 } else { n >= 0.0 },
            (None, Expr::Ternary(_, yes, no, _)) => {
                self.points(yes, scope, positive) && self.points(no, scope, positive)
            }
            (None, Expr::Str(..) | Expr::Bool(..)) => false,
            (None, value) => matches!(
                contract_types::infer(value, scope, &self.types.shapes),
                Ok(Ty::Number) | Err(_)
            ),
        }
    }

    /// The rules that make a list's axis CSS's and keep its index's starts
    /// prefix sums (LLP 1070 H2, §8, as amended for #226): a row list is a
    /// flex row of a definite height, left to right, that neither wraps,
    /// reverses nor spreads its items; its `gap` is a number of points the
    /// index counts between items. A vertical list is a block, where CSS lays
    /// out no gap.
    fn collection_axis(
        &self,
        attrs: &[Attr],
        row: bool,
        span: Span,
        scope: &Scope,
    ) -> Result<(), LowerError> {
        if let Some(display) = literal(attrs, "display") {
            if !matches!(string(display), Some("block" | "flex")) {
                return err("lower-collection-flow", "a virtualized list's `display` is a literal `block` (it scrolls vertically) or `flex` (a row that scrolls horizontally)", display.span);
            }
        }
        if let Some(direction) = literal(attrs, "flex-direction") {
            if !row {
                return err("lower-collection-flow", "`flex-direction` needs `display=\"flex\"` on a virtualized list; CSS ignores it on a block, and the list would silently scroll vertically", direction.span);
            }
            if string(direction) != Some("row") {
                return err("lower-collection-flow", "a virtualized flex list is a `flex-direction: row`; a reversed row is an inverted list, and a column is `display: block`", direction.span);
            }
        }
        let [own, other] = if row {
            ["estimated-item-width", "estimated-item-height"]
        } else {
            ["estimated-item-height", "estimated-item-width"]
        };
        if let Some(estimate) = literal(attrs, other) {
            return err("lower-collection-estimate", format!("`{other}` estimates the other axis; this list scrolls {}, so its estimate is `{own}`", if row { "horizontally" } else { "vertically" }), estimate.span);
        }
        if let Some(restoration) = literal(attrs, "scroll-restoration") {
            if !matches!(string(restoration), Some("auto" | "manual")) {
                return err("lower-attr-value", "`scroll-restoration` is `auto` (a nested list keeps where its reader left it) or `manual` (the app does)", restoration.span);
            }
        }
        if let Some(start) = literal(attrs, "scroll-start") {
            if !matches!(string(start), Some("start" | "end")) {
                return err("lower-attr-value", "`scroll-start` is `start` (the default) or `end` (the list opens at its last row)", start.span);
            }
        }
        if let Some(reorder) = literal(attrs, "reorderdrop").filter(|_| row) {
            return err("lower-collection-reorder", "reordering is vertical; a virtualized row list refuses `reorderdrop` until a consumer needs it (LLP 1070 §4.7)", reorder.span);
        }
        for gap in attrs
            .iter()
            .filter(|a| matches!(a.name.as_str(), "gap" | "row-gap" | "column-gap"))
        {
            if !row && numeric_literal(&gap.value) != Some(0.0) {
                return err("lower-collection-flow", format!("`{}` on a vertical virtualized list: it is a block, where CSS lays out no gap; space rows with a `margin-bottom` on the row root", gap.name), gap.span);
            }
            if row && !self.points(&gap.value, scope, false) {
                return err("lower-collection-flow", format!("`{}` on a virtualized row list is a nonnegative number of points, a literal or a computed number, which the list counts between its items", gap.name), gap.span);
            }
        }
        if !row {
            return Ok(());
        }
        if !literal(attrs, "height").is_some_and(|a| self.points(&a.value, scope, true)) {
            return err("lower-collection-cross", "a virtualized row list needs a `height` in points, a literal or a computed number: an auto height would be its tallest mounted item, which changes as items mount", span);
        }
        for a in attrs {
            let refusal = match a.name.as_str() {
                "flex-wrap" if string(a) != Some("nowrap") => Some("a wrapping virtualized list is a grid, which is not windowed"),
                "justify-content" if !matches!(string(a), Some("flex-start" | "normal")) => Some("main-axis alignment would move the items' starts; space items with a margin on the row root"),
                "direction" if string(a) == Some("rtl") => Some("a right-to-left virtualized row list is not windowed yet; its `scrollLeft` counts from the right edge"),
                _ => None,
            };
            if let Some(reason) = refusal {
                return err(
                    "lower-collection-flow",
                    format!("`{}` on a virtualized row list: {reason}", a.name),
                    a.span,
                );
            }
        }
        Ok(())
    }

    /// Refuses a flow root of fixed content that could overlap the data rows;
    /// `check_collection` finds the roots.
    fn check_fixed_root(&self, tag: &str, attrs: &[Attr], span: Span) -> Result<(), LowerError> {
        if tag == "dialog" {
            return err("lower-collection-fixed-flow", "a fixed collection root must participate in normal flow; move the dialog outside the list", span);
        }
        let expanded = self.with_class_rows(attrs);
        if literal(&expanded, "display").and_then(string) == Some("contents") {
            return Ok(());
        }
        for a in &expanded {
            let allowed = match a.name.as_str() {
                "position" => matches!(string(a), Some("static" | "relative")),
                "margin" | "margin-top" | "margin-bottom" | "margin-left" | "margin-right" => {
                    match super::shorthands::sides(&a.name, &a.value)? {
                        Some(sides) => sides.iter().all(fixed_margin),
                        None => fixed_margin(&a.value),
                    }
                }
                "top" | "bottom" => fixed_offset(&a.value),
                "inset" => {
                    let sides = super::shorthands::sides(&a.name, &a.value)?;
                    [0, 2].into_iter().all(|i| {
                        let value = sides.as_ref().map_or(&a.value, |s| &s[i]);
                        fixed_offset(value)
                    })
                }
                _ => true,
            };
            if !allowed {
                return err("lower-collection-fixed-flow", format!("`{}` on a fixed collection root can overlap the data rows; use static or relative positioning without vertical offsets, nonnegative margin lengths (including percentages and font-relative units), or move it outside the list", a.name), a.span);
            }
        }
        Ok(())
    }

    // Component uses and slots have already expanded before lowering. Inspect
    // all arms, even currently inactive ones. A row may hold one virtualized
    // list, one level down, whose lifetime is the row's (LLP 1070 N1, N6); it
    // may not hold another, reorder, or scroll vertically without a literal
    // height, since a row's height is its content's and `flex` alone bounds
    // nothing there.
    fn check_nested(&self, nodes: &[Node], inner: bool) -> Result<(), LowerError> {
        for node in nodes {
            match node {
                Node::Element {
                    attrs,
                    children,
                    span,
                    ..
                } => {
                    // `virtualized` is a prop, which no `style` may hold.
                    let opt = attrs.iter().find(|a| a.name == "virtualized");
                    if let Some(opt) = opt.filter(|a| matches!(a.value, Expr::Bool(true, _))) {
                        if inner {
                            return err("lower-collection-depth", "virtualized lists nest one level deep: this list is inside a virtualized list that is itself in a virtualized list's row", opt.span);
                        }
                        if let Some(reorder) = attrs.iter().find(|a| a.name == "reorderdrop") {
                            return err("lower-collection-reorder", "reordering is not built for a virtualized list in another's row (LLP 1070 §4.7)", reorder.span);
                        }
                        let bounded = attrs.iter().any(|a| {
                            matches!(a.name.as_str(), "height" | "max-height")
                                && numeric_literal(&a.value).is_some_and(|n| n > 0.0)
                        });
                        if !row_list(attrs) && !bounded {
                            return err("lower-collection-unbounded", "a virtualized list in a virtualized list's row needs a literal `height` or `max-height`: the row's height is its content's, so `flex` bounds nothing there", *span);
                        }
                        self.check_nested(children, true)?;
                        continue;
                    }
                    self.check_nested(children, inner)?;
                }
                node => {
                    for list in node.child_lists() {
                        self.check_nested(list, inner)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Main-axis padding on a virtualized list (@ref LLP 1010 §6.9): CSS's
    /// room before the first row and after the last, inside the scroll
    /// content. It takes what padding takes elsewhere (a number,
    /// `env(safe-area-inset-*)`, `calc(env(…) ± px)`), computed too: each
    /// host reports the padding it laid out with the list's offset.
    /// A percentage is refused: Apple's hosts place rows from the authored
    /// padding, which has no containing block to resolve one against.
    /// `scroll-padding` along the axis takes the same forms: the inset a
    /// `scrollIntoView` aligns within, which the runner reads from the
    /// list's style when the request is made.
    fn collection_inset(&self, attrs: &[Attr], row: bool, scope: &Scope) -> Result<(), LowerError> {
        let (ends, sides) = if row {
            (["left", "right"], [3, 1])
        } else {
            (["top", "bottom"], [0, 2])
        };
        let main_axis = |name: &str| {
            ["padding", "scroll-padding"].into_iter().any(|family| {
                name == family || ends.iter().any(|end| name == format!("{family}-{end}"))
            })
        };
        for a in attrs.iter().filter(|a| main_axis(&a.name)) {
            let main = match super::shorthands::sides(&a.name, &a.value)? {
                Some(four) if !a.name.ends_with(ends[0]) && !a.name.ends_with(ends[1]) => {
                    sides.map(|i| four[i].clone()).to_vec()
                }
                _ => vec![a.value.clone()],
            };
            for value in &main {
                self.inset_value(a, value, scope)?;
            }
        }
        Ok(())
    }
    fn inset_value(&self, a: &Attr, value: &Expr, scope: &Scope) -> Result<(), LowerError> {
        match value {
            Expr::Str(s, _) if s.contains('%') => err("lower-collection-flow", format!("`{}` on a virtualized list is a length: a percentage resolves against the containing block's width, which the list's windowing does not follow; write points or `env(safe-area-inset-*)`", a.name), a.span),
            Expr::Str(..) | Expr::Number(..) => Ok(()),
            Expr::Ternary(_, yes, no, _) => {
                self.inset_value(a, yes, scope)?;
                self.inset_value(a, no, scope)
            }
            _ => match contract_types::infer(value, scope, &self.types.shapes) {
                Ok(Ty::Number) => Ok(()),
                // The value's own binding names a type error.
                Err(_) => Ok(()),
                Ok(_) => err("lower-collection-flow", format!("a computed `{}` on a virtualized list is a number (points): a computed string could be a percentage, which the list's windowing does not follow; choose between literal lengths instead", a.name), a.span),
            },
        }
    }

    fn collection_flow(&self, attrs: &[Attr], row: Option<bool>) -> Result<(), LowerError> {
        let container = row.is_some();
        let horizontal = row == Some(true);
        for a in attrs {
            let allowed = match a.name.as_str() {
                "position" => {
                    matches!(&a.value, Expr::Str(s, _) if s == "relative" || s == "static")
                }
                "margin" | "margin-top" | "margin-bottom" => {
                    numeric_literal(&a.value).is_some_and(|v| v >= 0.0)
                }
                "display" if container => true, // `collection_axis`
                "flex-direction" | "flex-wrap" | "justify-content" | "direction" if horizontal => {
                    true // `collection_axis`
                }
                "display" => !matches!(&a.value, Expr::Str(s, _) if s == "contents"),
                "overflow" if container => {
                    matches!(&a.value, Expr::Str(s, _) if s == "scroll" || s == "auto")
                }
                // The main axis scrolls; the cross axis is `hidden`, as a
                // vertical list's `overflow-x` has always been.
                "overflow-y" | "overflow-x" if container => {
                    let main = if horizontal {
                        "overflow-x"
                    } else {
                        "overflow-y"
                    };
                    matches!(&a.value, Expr::Str(s, _) if if a.name == main { s == "scroll" || s == "auto" } else { s == "hidden" })
                }
                "gap" | "row-gap" | "column-gap" if container => true, // `collection_axis`
                "flex-direction" | "flex-wrap" | "grid-template-rows" | "grid-template-columns"
                    if container =>
                {
                    false
                }
                _ => true,
            };
            if !allowed
                && a.name == "position"
                && matches!(&a.value, Expr::Str(s, _) if s == "sticky")
            {
                // @ref LLP 1083 D5 — each row is laid out in a wrapper of its own.
                return err("lower-collection-flow", "a windowed row's root would stick only inside its own row: make a section one row, and its header `position: sticky` inside it", a.span);
            }
            if !allowed {
                return err("lower-collection-flow", format!("`{}` is not supported on this virtual collection flow root; absolute/overlapping rows and alternate container layouts are not windowed", a.name), a.span);
            }
        }
        Ok(())
    }
}
