//! A new host under the same presenter (a development reload, an activated
//! update): what belongs to the old tree goes, and what belongs to the
//! window or the session stays.
//!
//! The new runner numbers its views from 1 again and its kernel counts its
//! epoch from where a boot does, so state kept by view id, node key, epoch
//! or plan index would name another node, or none. [`Presenter::replaced`]
//! names every field: a field added later does not compile until it is
//! said to be the tree's or kept.
use super::*;

impl<D: DataSource> Presenter<D> {
    /// The host was replaced; the caller then restores the focus by its
    /// place ([`Presenter::restore_focus`]), which reads the old one.
    pub(crate) fn replaced(&mut self) {
        let Presenter {
            // The tree's, made anew here.
            deferred_collections,
            last_wheel,
            idle_timer_retry,
            ahead,
            scroll,
            scroll_end,
            wheel_free,
            snap_rested,
            glides,
            page,
            focus_out,
            autofocus_from,
            disabled_blur,
            edited,
            fields,
            field_history,
            field_drag,
            range_drag,
            ime,
            window_focus,
            controls,
            chosen,
            hovered,
            pointer_held,
            pointer_path,
            pads,
            control_bindings,
            control_contact,
            parked,
            collection,
            contact,
            retained_motion,
            arrange,
            group,
            transform_geometry,
            commands,
            fragment_armed,
            fragment_seen,
            fragment_anchor,
            focus_start,
            field_select_tasks,
            field_change_tasks,
            bars,
            stage,
            // The tree's, made anew by the calls below.
            surfaces: _,
            brush: _,
            ranges: _,
            menu: _,
            menu_row: _,
            menu_scroll: _,
            requests: _,
            top: _,
            alert: _,
            hatches: _,
            // The tree's, set by `restore_focus` from the place of the old
            // focus.
            focus: _,
            autofocus_processed: _,
            route_focus: _,
            // Set by the caller: the new host, and what came with it.
            host: _,
            executor: _,
            module: _,
            text: _,
            images: _,
            assets: _,
            painted: _,
            activation_failed: _,
            display: _,
            // Counts and the ids they give: never given again.
            hosts: _,
            clock_generation: _,
            ime_epoch: _,
            person_scrolls: _,
            // Replaced by the next frame's paint.
            boxes: _,
            boxes_page: _,
            boxes_serial: _,
            dirty: _,
            image_paint_pending: _,
            field_reveal: _,
            last_frame_succeeded: _,
            last_region_frame: _,
            last_region_scale: _,
            // What the shell was told, which the next statement corrects
            // where the new tree differs.
            reader: _,
            colors: _,
            hosted: _,
            access: _,
            // The window's, the device's and the session's.
            live_clock: _,
            native_timers: _,
            timers_hidden: _,
            timer_observations: _,
            dev: _,
            pending_dev: _,
            pending_update: _,
            viewport: _,
            compat: _,
            window_active: _,
            clipboard: _,
            url_opener: _,
            window_focuser: _,
            window_hider: _,
            window_closer: _,
            portal: _,
            held: _,
            shortcut_keys: _,
            key_text: _,
            pointer: _,
            hover_point: _,
            press_point: _,
            pointer_walking: _,
            pointer_buttons: _,
            scheme: _,
            device: _,
            text_scale: _,
            pointer_kind: _,
            slop: _,
            window: _,
            segments: _,
            choice: _,
            fonts_ms: _,
            painter: _,
            refusal_turn: _,
            updates: _,
            agent: _,
            booting: _,
            content_registration: _,
        } = self;
        // Turns still wait for the frame; none of the old tree's do.
        if let Some(waiting) = deferred_collections {
            waiting.clear();
        }
        *last_wheel = None;
        *idle_timer_retry = false;
        *ahead = None;
        scroll.clear();
        *scroll_end = Default::default();
        // Kept by view id, as where a wheel left a scroller and the area it
        // last rested on.
        wheel_free.clear();
        snap_rested.clear();
        *glides = Default::default();
        *page = (0.0, 0.0);
        focus_out.clear();
        *autofocus_from = None;
        disabled_blur.clear();
        // An unbound control's own state, a choice or typed text (LLP
        // 1069.001 D4), an edit to commit, a caret: as Apple's reset and
        // the web's rebuilt page have it.
        *edited = None;
        fields.clear();
        *field_history = Default::default();
        *field_drag = None;
        *range_drag = None;
        *ime = None;
        *window_focus = None;
        controls.clear();
        chosen.clear();
        hovered.clear();
        *pointer_held = None;
        pointer_path.clear();
        pads.replaced();
        control_bindings.clear();
        *control_contact = None;
        parked.clear();
        *collection = Default::default();
        *contact = None;
        *retained_motion = None;
        *arrange = None;
        *group = None;
        *transform_geometry = Default::default();
        commands.clear();
        // The new runner counts its router's activations from the start.
        *fragment_armed = None;
        *fragment_seen = 0;
        *fragment_anchor = None;
        *focus_start = None;
        field_select_tasks.clear();
        field_change_tasks.clear();
        bars.replaced();
        // Sheets, a crossing and a preview are the old tree's routes.
        *stage = Default::default();
        self.read_ranges();
        self.give_clock();
        self.modules_replaced();
        self.brush.replaced();
        self.brush.lift = Default::default();
        // A picker's answer or a GPU surface is not the new tree's.
        self.picks_replaced();
        self.close_menu();
        self.surfaces.replaced(&mut self.host);
        self.hatch_reset();
        self.top_replaced();
        self.holds_changed();
        self.back_changed();
        self.hosts += 1;
        self.measure();
    }
}
