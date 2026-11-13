/// What a redirect response does: the Fetch standard's `redirect` option.
/// Native transports follow it in Rust, re-checking the grant on every hop;
/// `Manual` hands the 3xx back (its `Location` readable), `Error` fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Redirect {
    /// Follow to the final response.
    #[default]
    Follow,
    /// Return the redirect response itself.
    Manual,
    /// Fail the request on a redirect.
    Error,
}

impl Redirect {
    /// The mode `fetch` names (absent is `follow`), refusing any other word.
    pub fn parse(name: Option<&str>) -> Result<Redirect, String> {
        match name.unwrap_or("follow") {
            "follow" => Ok(Redirect::Follow),
            "manual" => Ok(Redirect::Manual),
            "error" => Ok(Redirect::Error),
            other => Err(format!("redirect: follow, manual or error, not {other}")),
        }
    }

    /// Its name as `fetch` spells it.
    pub fn name(self) -> &'static str {
        match self {
            Redirect::Follow => "follow",
            Redirect::Manual => "manual",
            Redirect::Error => "error",
        }
    }
}

/// Fetch's `RequestCache`, used by the executor or the browser (LLP 1162 D4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CacheMode {
    /// The author left the mode unsaid; a long poll uses no-store.
    #[default]
    Unspecified,
    /// Use fresh replies and revalidate stale ones.
    Default,
    /// Neither read nor write the cache.
    NoStore,
    /// Ask the network and store the reply.
    Reload,
    /// Revalidate a stored reply before using it.
    NoCache,
    /// Use a stored reply whatever its age, or ask the network.
    ForceCache,
    /// Use a stored reply whatever its age, or fail without asking the network.
    OnlyIfCached,
}

impl CacheMode {
    /// Fetch's spelling; absent is `default`.
    pub fn parse(name: Option<&str>) -> Result<Self, String> {
        match name {
            None => Ok(Self::Unspecified),
            Some("default") => Ok(Self::Default),
            Some("no-store") => Ok(Self::NoStore),
            Some("reload") => Ok(Self::Reload),
            Some("no-cache") => Ok(Self::NoCache),
            Some("force-cache") => Ok(Self::ForceCache),
            Some("only-if-cached") => Ok(Self::OnlyIfCached),
            Some(other) => Err(format!("invalid cache mode: {other}")),
        }
    }

    /// Its name as `fetch` spells it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Unspecified | Self::Default => "default",
            Self::NoStore => "no-store",
            Self::Reload => "reload",
            Self::NoCache => "no-cache",
            Self::ForceCache => "force-cache",
            Self::OnlyIfCached => "only-if-cached",
        }
    }
}

impl super::Request {
    /// Set the prefetch option; currently a plain fetch on every host (LLP 1162.000 D5).
    pub fn prefetch(mut self) -> Self {
        self.prefetch = true;
        self
    }

    /// The executor also checks the request's read class at admission.
    pub fn prefetch_refusal(&self) -> Option<&'static str> {
        (self.prefetch && !self.is_plain_http())
            .then_some("exactPrefetch requires a plain HTTP read")
    }

    /// Name a preload; on the web it is accepted and ignored (LLP 1162.000 D1).
    pub fn preload(mut self, name: &str) -> Result<Self, String> {
        self.preload = Some(name.into());
        match self.preload_refusal() {
            Some(why) => Err(why.into()),
            None => Ok(self),
        }
    }

    /// Refuse an invalid name even when a source set the field directly.
    pub fn preload_refusal(&self) -> Option<&'static str> {
        self.preload.as_ref().and_then(|name| {
            let mut bytes = name.bytes();
            (!(bytes.next().is_some_and(|b| b.is_ascii_lowercase())
                && bytes.all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')))
            .then_some("exactPreload must match ^[a-z][a-z0-9-]*$")
        })
    }
}
