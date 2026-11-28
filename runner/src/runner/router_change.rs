//! What a commit that changed the router tells a host: the router seam's published change (LLP 1038 D7).

/// The selected visit and all visit ids removed from any retained tab.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouterChange {
    /// The selected top's visit id.
    pub top: u64,
    /// Its canonical location.
    pub url: String,
    /// Its fragment, `""` when it has none (LLP 1153 D5).
    pub fragment: String,
    /// Whether the host scrolls to that fragment once the route's content
    /// has arrived: the top is a new visit with a fragment, its fragment
    /// changed, or the router was assigned a navigation to it. A return to a
    /// visit (Back, a tab switch) keeps its place.
    pub scroll: bool,
    /// That scroll's activation, counted from 1 for the router's life, `0`
    /// with none: the third argument of the `scrollToFragment` that makes
    /// it, so a host matches the two and takes each once.
    pub activation: u64,
    /// Removed ids, in the old value's tab/stack order.
    pub removed: Vec<u64>,
    /// Every retained visit's canonical location, in tab/stack order.
    pub visits: Vec<(u64, String)>,
    /// Each tab's root visit, in tab order: a tab switch from a step in one.
    pub roots: Vec<u64>,
    /// The selected tab's name: the top's tab.
    pub tab: String,
    /// What the route table resolves the top's location to (`Table::chain`).
    pub chain: Vec<exact_route::Destination>,
    /// The top's route pattern (`/post/:id`), which navigation metrics group
    /// by. Empty when the table has no such route.
    pub pattern: String,
    /// The top's bound parameters, in the table's order.
    pub params: Vec<(String, String)>,
    /// Whether work that fell due (a timer, a wake) made the commit that put
    /// the top in place, even inside an input's dispatch, where overdue work
    /// runs before the input's own action. A host times such a navigation as
    /// the program's, from its commit (LLP 1132 D1, "Cause").
    pub due: bool,
}

/// What an input's own dispatch did, told to the callback the input carries
/// ([`crate::Runner::dispatch_at_with_receipt`]) when the runner refuses it on
/// arrival or once it dispatches it: at once, or later for an input held
/// behind a change action's yield. A refusal commits nothing. A successful dispatch names the selected top its
/// commit left, with its canonical location, when the commit opened that
/// visit or moved it to another location. A return to a visit as it was
/// (Back, a tab switch), a new fragment of the same visit and an assignment of
/// an equal router open none, and so does a visit the commit opened or moved
/// in a stack it did not leave selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputCommit {
    /// The runner refused the event, or the time it arrived at (NaN, before it
    /// was held), so nothing of it committed
    Refused,
    /// The runner dispatched the event: its handler committed, or no listener
    /// hears an event the view accepts, and the receipt is empty
    Committed {
        /// The selected top its commit opened or moved, by id, and its canonical location
        opened: Option<(u64, String)>,
    },
}

impl RouterChange {
    /// The top's location as a host writes it back: its url, then `#` and
    /// its fragment when it has one.
    pub fn location(&self) -> String {
        if self.fragment.is_empty() {
            self.url.clone()
        } else {
            format!("{}#{}", self.url, self.fragment)
        }
    }
}
