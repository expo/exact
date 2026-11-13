//! A list's `gap` and row estimate, read again as they change (LLP 1070, as
//! amended for #226). A row list's `column-gap` is CSS's space between its
//! items: the index counts it after each row, so a row's extent is its size
//! and the gap after it, and the rows end one gap before their extents'
//! sum. A vertical list is a block, where CSS lays out no gap.
use super::*;

/// What a list's options say of its sizes: its row estimate and its gap.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sizing {
    pub(crate) estimate: f64,
    pub(crate) gap: f64,
}

impl Sizing {
    /// From the values bound to the list's estimate and to its main-axis
    /// gap, as the list is created: an estimate that is not a positive
    /// number refuses the list; a gap that is not a nonnegative number is
    /// none.
    pub(crate) fn read(
        axis: ListAxis,
        estimate: Option<Value>,
        gap: Option<Value>,
    ) -> Result<Self, InstanceError> {
        let estimate = match estimate {
            None => ESTIMATED_HEIGHT,
            Some(Value::Number(n)) if n.is_finite() && n > 0.0 => n,
            Some(Value::Number(_)) => {
                return Err(invalid("estimated item height must be positive and finite"))
            }
            Some(_) => return Err(invalid("estimated item height must be a number")),
        };
        Ok(Self {
            estimate,
            gap: Self::gap(axis, gap.as_ref()),
        })
    }

    fn gap(axis: ListAxis, value: Option<&Value>) -> f64 {
        match value {
            Some(Value::Number(n)) if axis == ListAxis::Horizontal && n.is_finite() && *n > 0.0 => {
                *n
            }
            _ => 0.0,
        }
    }

    /// A row's extent before it is measured: the estimate and the gap after it.
    pub(crate) fn extent(self) -> f64 {
        self.estimate + self.gap
    }
}

impl Collection {
    /// The gap after a row in its extent: the list's, but none after a
    /// mounted row whose root is not displayed, which CSS lays no gap around.
    pub(super) fn gap_after(&self, row: Option<&Mounted>) -> f64 {
        if row.is_some_and(|r| r.undisplayed) {
            0.0
        } else {
            self.sizing.gap
        }
    }

    /// A mounted row's wrapper follows its root's `display: none` in a list
    /// with a gap (CSS Flexbox 1 §4: an item that is not displayed is no
    /// flex item, and no gap is laid out around it). The root's value is the
    /// one its binding last emitted.
    pub(super) fn sync_undisplayed(&mut self, u: &mut Update<'_>) -> Result<(), InstanceError> {
        let gapped = self.sizing.gap > 0.0;
        for row in &mut self.mounted {
            let undisplayed = gapped
                && matches!(&row.row.roots[..], [Child::Node(root)]
                    if root.bound_style(u.env.plan, u.sites, exact_kernel::StyleId::Display).as_ref().and_then(Value::as_str) == Some("none"));
            if row.undisplayed != undisplayed {
                row.undisplayed = undisplayed;
                let display = if undisplayed { "none" } else { "flex" };
                views::style(u, row.wrapper, &[("display", Value::str(display))])?;
            }
        }
        Ok(())
    }

    /// The estimate or the gap changed (a computed one): `estimate`, the
    /// value bound to the estimate now, and `gap`, to the main-axis gap. A
    /// new estimate is what every row the list has not mounted takes, its
    /// measurement taken under the old one dropped, as an eager list lays
    /// every row out again; a mounted row keeps its size until the host
    /// measures it again. A new gap moves every row after the first by it.
    /// The list keeps what shows. An estimate that is not a positive number
    /// changes nothing.
    pub(super) fn resize(
        &mut self,
        u: &mut Update<'_>,
        frames: &[Frame],
        estimate: Option<&Value>,
        gap: Option<&Value>,
    ) -> Result<(), InstanceError> {
        let estimate = match estimate {
            Some(&Value::Number(n)) if n.is_finite() && n > 0.0 => n,
            _ => self.sizing.estimate,
        };
        let next = Sizing {
            estimate,
            gap: Sizing::gap(self.axis, gap),
        };
        if next == self.sizing {
            return Ok(());
        }
        self.end_preview(u)?;
        let anchor = self.anchor()?;
        let kept: Vec<usize> = if next.estimate == self.sizing.estimate {
            (0..self.index.len()).collect()
        } else {
            self.mounted.iter().map(|m| m.position).collect()
        };
        self.index
            .resize(next.extent(), next.gap, &kept)
            .map_err(index_error)?;
        self.sizing = next;
        self.estimated_height = next.extent();
        self.zero_heights.retain(|key| {
            self.index
                .position(key)
                .is_some_and(|p| kept.binary_search(&p).is_ok())
        });
        self.restore_unless_unknown(anchor)?;
        self.realize_window(u, frames, false, CollectionFill::default())?;
        advance(&mut self.revision)?;
        Ok(())
    }
}

impl Collection {
    pub(crate) fn create(
        u: &mut Update<'_>,
        node: NodesId,
        view: ViewId,
        frames: &[Frame],
    ) -> Result<Option<Box<Self>>, InstanceError> {
        let plan = u.env.plan;
        let descriptor = plan.node(node);
        let mut enabled = false;
        let mut follow_end = false;
        let mut smooth = false;
        let mut at_end = false;
        let mut reach = reach::Margin::default();
        let mut keep = Vec::new();
        let (mut estimate, mut gap) = (None, None);
        let mut initial: Option<usize> = None;
        let mut axis = ListAxis::Vertical;
        // The list's own literal size on each axis (`height=399`), which
        // bounds the rows its first frame needs: [vertical, horizontal].
        let mut declared: [Option<f64>; 2] = [None, None];
        for binding in descriptor.bindings.iter().map(|b| plan.binding(b)) {
            if binding.kind == BindingKind::Style {
                use exact_kernel::StyleId;
                let slot = match binding.id {
                    id if id == StyleId::Height as u16 || id == StyleId::MaxHeight as u16 => {
                        Some(0)
                    }
                    id if id == StyleId::Width as u16 || id == StyleId::MaxWidth as u16 => Some(1),
                    _ => None,
                };
                if let Some(slot) = slot {
                    if let Value::Number(n) = u.eval_list(&binding, frames, view, node)? {
                        if n.is_finite() && n > 0.0 {
                            declared[slot] = Some(declared[slot].map_or(n, |d: f64| d.min(n)));
                        }
                    }
                }
            }
            // A flex list is CSS's row (the compiler refuses the other
            // directions); its main axis is horizontal.
            if binding.kind == BindingKind::Style
                && binding.id == exact_kernel::StyleId::Display as u16
                && u.eval_list(&binding, frames, view, node)?.as_str() == Some("flex")
            {
                axis = ListAxis::Horizontal;
            }
            if binding.kind == BindingKind::Style
                && binding.id == exact_kernel::StyleId::ScrollBehavior as u16
                && u.eval_list(&binding, frames, view, node)?.as_str() == Some("smooth")
            {
                smooth = true;
            }
            if binding.kind == BindingKind::Prop && binding.id == PropId::Virtualized as u16 {
                enabled = u.eval_list(&binding, frames, view, node)? == Value::Bool(true);
            }
            if binding.kind == BindingKind::Prop && binding.id == PropId::ScrollFollowEnd as u16 {
                follow_end = u.eval_list(&binding, frames, view, node)? == Value::Bool(true);
            }
            if binding.kind == BindingKind::Prop && binding.id == PropId::ReachMargin as u16 {
                let margin = u.eval_list(&binding, frames, view, node)?;
                reach = margin
                    .as_str()
                    .and_then(reach::Margin::parse)
                    .ok_or_else(|| {
                        invalid("reach margin must be one or two nonnegative px or % lengths")
                    })?;
            }
            if binding.kind == BindingKind::Prop && binding.id == PropId::KeepMounted as u16 {
                keep =
                    keep::idents(&u.eval_list(&binding, frames, view, node)?).unwrap_or_default();
            }
            if binding.kind == BindingKind::Prop && binding.id == PropId::ScrollStart as u16 {
                at_end = u.eval_list(&binding, frames, view, node)?.as_str() == Some("end");
            }
            if binding.kind == BindingKind::Prop
                && (binding.id == PropId::EstimatedItemHeight as u16
                    || binding.id == PropId::EstimatedItemWidth as u16)
            {
                estimate = Some(u.eval_list(&binding, frames, view, node)?);
            }
            if binding.kind == BindingKind::Style
                && binding.id == exact_kernel::StyleId::ColumnGap as u16
            {
                gap = Some(u.eval_list(&binding, frames, view, node)?);
            }
            if binding.kind == BindingKind::Prop && binding.id == PropId::InitialItemCount as u16 {
                let Value::Number(count) = u.eval_list(&binding, frames, view, node)? else {
                    return Err(invalid("initial item count must be a number"));
                };
                if !(count.fract() == 0.0 && (1.0..=64.0).contains(&count)) {
                    return Err(invalid(
                        "initial item count must be a whole number from 1 to 64",
                    ));
                }
                initial = Some(count as usize);
            }
        }
        if !enabled {
            return Ok(None);
        }
        if descriptor.node_type != exact_kernel::NodeType::List as u8 {
            return Err(invalid("virtualized requires List"));
        }
        let child_sites = u.sites.children(Some(node), descriptor.arm);
        let [(_, Site::Region(region))] = child_sites else {
            return Err(invalid("collection needs one direct `for`"));
        };
        let region = *region;
        let row = plan.region(region);
        if row.kind != RegionKind::Each || row.arms.len != 1 {
            return Err(invalid("collection needs one `for` arm"));
        }
        if !matches!(
            u.sites.children(None, row.arms.iter().next()),
            [(_, Site::Node(_))]
        ) {
            return Err(invalid("collection row needs one flow root"));
        }
        let sizing = Sizing::read(axis, estimate, gap)?;
        let estimated_height = sizing.extent();
        let inner = traversal::validate_nesting(plan, u.sites, region)?;
        let reuse = reuse::Reuse::new(plan, u.sites, region);
        let nested = inner.is_some();
        let port = declared[match axis {
            ListAxis::Vertical => 0,
            ListAxis::Horizontal => 1,
        }];
        // A list's reads of its own options are its structure, as a region's
        // subject is: a trap here still poisons (LLP 1017.003 D3).
        let mut manual = false;
        for b in descriptor.bindings.iter().map(|b| plan.binding(b)) {
            if b.kind == BindingKind::Prop && b.id == PropId::ScrollRestoration as u16 {
                manual |= u.eval_list(&b, frames, view, node)?.as_str() == Some("manual");
            }
        }
        let mut this = Box::new(Self {
            // What a later row is in, not the visit's re-theme of the rest.
            around: super::theme::Around {
                retheme: false,
                ..u.theme.clone()
            },
            preview: None,
            pending_drops: Vec::new(),
            incoming: None,
            hidden: None,
            instant: false,
            view,
            axis,
            region,
            index: {
                let mut index = SizeIndex::new(estimated_height).map_err(index_error)?;
                index
                    .resize(estimated_height, sizing.gap, &[])
                    .map_err(index_error)?;
                index
            },
            estimated_height,
            sizing,
            // The provisional extent ([`set_bootstrap_extent`]), capped at
            // sixteen rows, unless the list says how many (`initial-item-count`).
            // Actual nested-scrollport feedback determines the real window.
            bootstrap_rows: initial.unwrap_or(
                ((f64::from_bits(BOOTSTRAP_EXTENT.load(std::sync::atomic::Ordering::Relaxed))
                    / estimated_height)
                    .ceil()
                    .min(
                        port.filter(|_| in_collection_row(plan, frames))
                            .map_or(f64::INFINITY, |p| (p / estimated_height).ceil() + 1.0),
                    )
                    .clamp(1.0, BOOTSTRAP_ROWS as f64)) as usize,
            ),
            declared_port: port,
            items: Items::default(),
            keys: Vec::new(),
            dups: BTreeMap::new(),
            string_keys: true,
            mounted: Vec::new(),
            any_awaiting: false,
            spacers: Vec::new(),
            children: Vec::new(),
            revision: 0,
            data_generation: 0,
            next_epoch: 0,
            zero_heights: Default::default(),
            geometry: None,
            correction: None,
            follow_end,
            smooth,
            edge_handlers: [EventKind::Reachstart, EventKind::Reachend].map(|event| {
                descriptor
                    .handlers
                    .iter()
                    .any(|h| plan.handler(h).event == event)
            }),
            reorderable: descriptor
                .handlers
                .iter()
                .any(|h| plan.handler(h).event == EventKind::Reorderdrop),
            edge_armed: [true; 2],
            edge_fired: [None, None],
            pending: false,
            parent: None,
            nested,
            inner,
            inner_pins: Default::default(),
            kept: Default::default(),
            manual,
            restored: false,
            restored_at: None,
            target: None,
            scrolling: false,
            animating: false,
            view_offset: None,
            port_unknown: false,
            request: 0,
            into_view_status: None,
            covered: [0.0; 2],
            start_offset: 0.0,
            reach,
            keep,
            at_end,
            end_travel: 0,
            end_sent: f64::NAN,
            reuse,
        });
        this.update_data(u, frames, true, false)?;
        Ok(Some(this))
    }
}
