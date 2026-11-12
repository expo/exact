//! Answers named by a waiting page (LLP 1048.007 D10), owned by its server.
use exact_plan::ActivatePolicy;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::{Arc, Mutex};

const MAX_CHECKPOINTS: usize = 512;
const MAX_BYTES: usize = 64 << 20;

#[derive(Default)]
pub(crate) struct Checkpoints(Mutex<Kept>);

#[derive(Default)]
struct Kept {
    answers: VecDeque<(String, Arc<str>)>,
    bytes: usize,
}

impl Checkpoints {
    pub(crate) fn insert(&self, text: &str) -> String {
        let hash = hex(text.as_bytes())[..32].to_string();
        let mut kept = self.0.lock().unwrap();
        if let Some(at) = kept.answers.iter().position(|(key, _)| *key == hash) {
            let answer = kept.answers.remove(at).unwrap();
            kept.answers.push_back(answer);
        } else {
            kept.bytes += text.len();
            kept.answers.push_back((hash.clone(), Arc::from(text)));
        }
        while kept.answers.len() > MAX_CHECKPOINTS || kept.bytes > MAX_BYTES {
            let (_, text) = kept.answers.pop_front().unwrap();
            kept.bytes -= text.len();
        }
        hash
    }

    pub(crate) fn get(&self, hash: &str) -> Option<Arc<str>> {
        let mut kept = self.0.lock().unwrap();
        let at = kept.answers.iter().position(|(key, _)| key == hash)?;
        let answer = kept.answers.remove(at).unwrap();
        let text = answer.1.clone();
        kept.answers.push_back(answer);
        Some(text)
    }
}

/// The checkpoint attribute and inline body. Only a request's writer gets a store.
pub(crate) fn checkpoint<'a>(
    text: &'a str,
    activate: ActivatePolicy,
    store: Option<&Checkpoints>,
) -> (String, &'a str) {
    match store.filter(|_| activate == ActivatePolicy::Interaction && text.len() > 1024) {
        Some(store) => (
            format!(
                " data-src=\"/.exact/checkpoint/{}.json\"",
                store.insert(text)
            ),
            "",
        ),
        None => (String::new(), text),
    }
}

pub(super) fn response(path: &str, store: &Checkpoints) -> Option<super::Response> {
    let hash = path
        .strip_prefix("/.exact/checkpoint/")?
        .strip_suffix(".json")?;
    if hash.len() != 32
        || !hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return None;
    }
    Some(match store.get(hash) {
        Some(text) => super::Response {
            status: 200,
            headers: vec![
                ("Content-Type", "application/json".into()),
                ("Cache-Control", "private, max-age=3600, immutable".into()),
            ],
            body: text.as_bytes().to_vec(),
            file: None,
            streamed: false,
        },
        None => super::Response::text(
            404,
            "These answers are no longer kept; the page builds itself afresh.\n",
        )
        .header("Cache-Control", "no-store"),
    })
}

pub(super) fn hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_large_interaction_checkpoints_with_a_server_are_named() {
        let store = Checkpoints::default();
        let large = "x".repeat(1025);
        for (text, activate, owner, deferred) in [
            (
                &large[..1024],
                ActivatePolicy::Interaction,
                Some(&store),
                false,
            ),
            (
                large.as_str(),
                ActivatePolicy::Interaction,
                Some(&store),
                true,
            ),
            (large.as_str(), ActivatePolicy::Idle, Some(&store), false),
            (
                large.as_str(),
                ActivatePolicy::Inferred,
                Some(&store),
                false,
            ),
            (large.as_str(), ActivatePolicy::Never, Some(&store), false),
            (large.as_str(), ActivatePolicy::Interaction, None, false),
        ] {
            let (src, body) = checkpoint(text, activate, owner);
            assert_eq!(!src.is_empty(), deferred);
            assert_eq!(body, if deferred { "" } else { text });
        }
    }

    #[test]
    fn making_or_asking_an_answer_keeps_it_last_within_both_budgets() {
        let store = Checkpoints::default();
        let first = store.insert("first");
        let second = store.insert("second");
        for i in 0..MAX_CHECKPOINTS - 2 {
            store.insert(&i.to_string());
        }
        assert_eq!(store.get(&first).as_deref(), Some("first"));
        store.insert("new");
        assert!(store.get(&second).is_none());
        assert_eq!(store.insert("first"), first);
        let bytes = store.0.lock().unwrap().bytes;
        store.insert("first");
        assert_eq!(store.0.lock().unwrap().bytes, bytes);
        assert_eq!(store.0.lock().unwrap().answers.len(), MAX_CHECKPOINTS);
        let oversized = store.insert(&"x".repeat(MAX_BYTES + 1));
        assert!(store.get(&oversized).is_none());
        assert_eq!(store.0.lock().unwrap().bytes, 0);
        assert!(store.0.lock().unwrap().answers.is_empty());
    }

    #[test]
    fn checkpoint_paths_match_the_javascript_servers_paths() {
        let store = Checkpoints::default();
        let hash = store.insert("{}");
        let path = format!("/.exact/checkpoint/{hash}.json");
        assert_eq!(response(&path, &store).unwrap().status, 200);
        let missing = format!("/.exact/checkpoint/{}.json", "0".repeat(32));
        let gone = response(&missing, &store).unwrap();
        assert_eq!(gone.status, 404);
        assert!(gone.headers.contains(&("Cache-Control", "no-store".into())));
        for invalid in [
            path.to_uppercase(),
            format!("{path}/"),
            path.replace(&hash, "a"),
            path.replace(&hash, &"g".repeat(32)),
        ] {
            assert!(response(&invalid, &store).is_none());
        }
        assert!(Checkpoints::default().get(&hash).is_none());
    }
}
