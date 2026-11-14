//! The web host's linked capabilities, above the core host.
//!
//! @ref LLP 1047 D3 (the generated entry names what the plan uses)
//! @ref LLP 1047 D4 (core crates stop depending on capabilities)
//!
//! `exact-web` names no capability's crate; each capability's adapter lives
//! here, over both. An app's generated entry names the ones its plan uses
//! with [`linked!`], and the linker drops the rest: an app that renders no
//! Markdown carries no `exact_markdown` code.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod animations;
pub mod auth;
pub mod backdrop;
pub mod collections;
pub mod dataset;
pub mod documents;
pub mod drag;
pub mod effects;
pub mod format;
pub mod geometry;
pub mod gradients;
pub mod grid;
pub mod inspection;
pub mod markdown;
pub mod materials;
pub mod motion;
pub mod picker;
pub mod router;
pub mod segments;
pub mod share;
pub mod surfaces;
pub mod tabs;
pub mod text_transform;
pub mod timelines;
pub mod wide_colors;

/// Shadow grammars, linked when a plan binds `box-shadow` or `text-shadow`.
pub mod shadows {
    /// Register the grammars (LLP 1047 D3).
    pub const fn link(mut linked: exact_web::Linked) -> exact_web::Linked {
        linked.shadows = Some(exact_kernel::style::link_shadows);
        linked
    }
}

/// Corner grammar, linked when a plan binds `corner-shape`.
pub mod corners {
    /// Register the grammar (LLP 1047 D3).
    pub const fn link(mut linked: exact_web::Linked) -> exact_web::Linked {
        linked.corners = Some(exact_kernel::style::link_corners);
        linked
    }
}

/// Rotation grammars, linked when a plan binds its angle or axis.
pub mod rotate {
    /// Register the grammars (LLP 1047 D3).
    pub const fn link(mut linked: exact_web::Linked) -> exact_web::Linked {
        linked.rotate = Some(exact_kernel::style::link_rotate);
        linked
    }
}

/// Text stroke grammar, linked when a plan binds its width or colour.
pub mod text_stroke {
    /// Register the grammar (LLP 1047 D3).
    pub const fn link(mut linked: exact_web::Linked) -> exact_web::Linked {
        linked.text_stroke = Some(exact_kernel::style::link_text_stroke);
        linked
    }
}

/// Palette grammar, linked when a plan binds `-exact-symbol-palette`.
pub mod symbol_palette {
    /// Register the grammar (LLP 1047 D3).
    pub const fn link(mut linked: exact_web::Linked) -> exact_web::Linked {
        linked.symbol_palette = Some(exact_kernel::style::link_symbol_palette);
        linked
    }
}

/// The [`exact_web::Linked`] of the named capabilities, as a constant:
/// `linked!(markdown)`. The generated entry writes one; nothing else should.
#[macro_export]
macro_rules! linked {
    ($($capability:ident),* $(,)?) => {{
        let linked = ::exact_web::Linked::CORE;
        $(let linked = $crate::$capability::link(linked);)*
        linked
    }};
}

/// Every capability: what a development build and a native renderer link
/// (LLP 1047 D7).
pub const ALL: exact_web::Linked = linked!(
    markdown,
    motion,
    collections,
    drag,
    surfaces,
    router,
    format,
    inspection,
    wide_colors,
    materials,
    backdrop,
    auth,
    share,
    documents,
    picker,
    timelines,
    text_transform,
    effects,
    animations,
    gradients,
    grid,
    shadows,
    corners,
    rotate,
    text_stroke,
    symbol_palette,
    geometry,
    segments,
    dataset,
    tabs
);
