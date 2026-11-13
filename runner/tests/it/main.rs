//! The runner's integration tests: one binary, so one link and one launch.
//! `tests/surface_record.rs` stays its own binary: it installs a counting
//! global allocator.

mod active_route;
mod app_module;
mod arguments;
mod canvas2d;
mod change_tasks;
mod children;
mod declared_source;
mod each;
mod failed_readers;
mod flow_agent;
mod font_axes;
mod font_keyframes;
mod foresee;
mod format;
mod format_locales;
mod frame_origin;
mod frames_agent;
mod height_binding;
mod incremental;
mod keep_mounted;
mod keep_sent;
mod launch_ask;
mod launch_stand_in;
mod list_layout;
mod listen;
mod live_tick;
mod live_timers;
mod long_list;
mod native_field_resize;
mod native_timers;
mod now_screen;
mod queued_then_origin;
mod reach_margin;
mod reorder_codec;
mod replay;
mod router;
mod rust_sockets;
mod secret_state;
mod sockets;
mod stream_mapper;
mod svg_lengths;
mod tabs;
mod transform_binding;
mod uploads;
mod uses;
mod viewport;

mod wait_timers;
