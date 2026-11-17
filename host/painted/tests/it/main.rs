//! The Linux host's integration tests that leave the font environment alone:
//! one binary. `tests/pinned/` holds the ones that pin the fixture font, so
//! neither changes the other's process environment.

mod accent;
mod animation;
mod arrange;
mod box_filter;
mod colors;
mod focus;
mod font_stack;
mod forma;
mod grouped_rows;
mod height;
mod height_binding;
mod hit_slop;
mod holds;
mod image;
mod list_rows;
mod live;
mod media_session;
mod presence;
mod svg;
mod timeline;
mod top_layer;
mod transform_binding;
mod transform_contact;
mod viewport;
