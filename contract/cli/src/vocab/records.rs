//! The attributes whose value is a named construct lowered to several props:
//! `metadata=MediaMetadata(…)` (LLP 1098 D1) and
//! `dataTransfer=DataTransfer(…)` (LLP 1094.000 D1).

use contract_lower::tags::AttrTarget;

/// What `vocab` says of one: its construct, the props it lowers to, and
/// what it does.
pub(super) struct Hoisted {
    pub construct: &'static str,
    pub props: &'static [&'static str],
    pub does: &'static str,
}

pub(super) fn of(a: &AttrTarget) -> Option<Hoisted> {
    match a {
        AttrTarget::MediaMetadata => Some(Hoisted {
            construct: "MediaMetadata(title=, artist=, album=, artwork=), each a string",
            props: &["mediaTitle", "mediaArtist", "mediaAlbum", "mediaArtwork"],
            does: "claims the media session (LLP 1098 D1)",
        }),
        AttrTarget::DataTransfer => Some(Hoisted {
            construct: "DataTransfer(text=, url=, files=[…]), each optional, one required: text a string, url an absolute http: or https: URL, files shipped assets/ files or app:/ paths",
            props: &["dataTransferText", "dataTransferUrl", "dataTransferFiles"],
            does: "what a `draggable=true` node's drag into another app carries (LLP 1094.000 D1)",
        }),
        _ => None,
    }
}

/// `vocab <name>`'s lines for one.
pub(super) fn detail(a: &AttrTarget) -> Vec<String> {
    of(a).map_or_else(Vec::new, |h| {
        vec![
            format!("record {}", h.construct),
            format!("props {}, its fields", h.props.join(", ")),
            h.does.into(),
        ]
    })
}
