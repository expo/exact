//! `keep-mounted` (@ref LLP 1010 §6.11): rows a list keeps mounted wherever
//! its window is, named by key. VoiceOver moves from a transcript's first
//! message to its last only if both are in the tree; a playing row keeps
//! its player.
use super::*;

impl Collection {
    /// Take the list's `keep-mounted` value; whether the rows it names
    /// changed. A value it refuses keeps none (the bridge journals it).
    pub(super) fn keep_mounted(&mut self, value: Option<&Value>) -> bool {
        let keys = value.and_then(idents).unwrap_or_default();
        let changed = keys != self.keep;
        self.keep = keys;
        changed
    }

    /// The positions of the kept rows the list holds.
    pub(super) fn kept_rows(&self) -> Vec<usize> {
        self.keep
            .iter()
            .filter_map(|key| self.index.position(key))
            .collect()
    }
}
