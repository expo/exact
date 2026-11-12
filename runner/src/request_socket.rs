//! WebSocket subprotocol names (LLP 1016.000).
use super::Request;

impl Request {
    /// Offer these protocols in preference order. Names are distinct RFC 7230
    /// tokens, never access tokens. The server echoes its selection.
    pub fn protocols(
        mut self,
        names: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, String> {
        self.protocols = names.into_iter().map(Into::into).collect();
        self.check_protocols()?;
        Ok(self)
    }

    /// Refuse protocols on work other than a WebSocket stream.
    pub fn protocols_refusal(&self) -> Option<&'static str> {
        (!self.protocols.is_empty()
            && (!self.stream
                || !self.socket_url()
                || self.listen.is_some()
                || self.long_poll.is_some()
                || self.upload_progress))
            .then_some("exactProtocols: only a WebSocket stream takes protocols")
    }

    fn socket_url(&self) -> bool {
        ["ws:", "wss:"].iter().any(|scheme| {
            self.url
                .get(..scheme.len())
                .is_some_and(|s| s.eq_ignore_ascii_case(scheme))
        })
    }

    /// Refuse malformed or repeated names and non-socket URLs before transport work.
    pub fn check_protocols(&self) -> Result<(), String> {
        if !self.protocols.is_empty() && !self.socket_url() {
            return Err("exactProtocols: only a WebSocket stream takes protocols".into());
        }
        for (i, name) in self.protocols.iter().enumerate() {
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
            {
                return Err(format!(
                    "invalid WebSocket protocol {name:?}: expected a non-empty RFC 7230 token"
                ));
            }
            if self.protocols[..i].contains(name) {
                return Err(format!("duplicate WebSocket protocol {name:?}"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocols_require_a_socket_url_and_a_stream_at_admission() {
        let why = "exactProtocols: only a WebSocket stream takes protocols";
        assert_eq!(
            Request::get("https://example.test")
                .protocols(["chat"])
                .unwrap_err(),
            why
        );
        let mut request = Request::get("wss://example.test")
            .protocols(["chat"])
            .unwrap();
        assert_eq!(request.protocols_refusal(), Some(why));
        request.stream = true;
        assert_eq!(request.protocols_refusal(), None);
        request.url = "https://example.test".into();
        assert_eq!(request.protocols_refusal(), Some(why));
        assert_eq!(request.check_protocols(), Err(why.into()));
        assert_eq!(
            Request::get("https://example.test").protocols_refusal(),
            None
        );
    }

    #[test]
    fn protocol_names_are_tokens_and_distinct() {
        let request = Request::get("wss://example.test")
            .protocols(["chat.v2", "Chat.v2", "!#$%&'*+-.^_`|~"])
            .unwrap();
        assert_eq!(request.protocols[0], "chat.v2");
        for name in ["", "chat v2", "chat/v2", "é", "a=b", "a,b", "a\r\nb"] {
            assert!(Request::get("wss://example.test")
                .protocols([name])
                .unwrap_err()
                .contains(&format!("{name:?}")));
        }
        assert!(Request::get("wss://example.test")
            .protocols(["chat", "chat"])
            .unwrap_err()
            .contains("duplicate"));
        assert!(Request::get("wss://example.test")
            .protocols(Vec::<String>::new())
            .is_ok());
    }
}
