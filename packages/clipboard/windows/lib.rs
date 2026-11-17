// @exact/clipboard's Windows path form (LLP 1133.006 D1, D2, D5, D7).
// Calls run to completion on the window thread, in the host's dispatch order.
// PNG sanitization finishes before opening or emptying the clipboard.
use exact_painted::package::{json, Context, Json, Package, Reply};
use exact_windows::clipboard::{Image, Item};
use std::io::Read;

pub struct Clipboard {
    context: Context,
    writes: u64,
}

type Refusal = (&'static str, String);
const MOST_FILE: u64 = 64 * 1024 * 1024;

fn refused(code: &'static str, message: &str) -> Refusal {
    (code, message.into())
}

fn prepare(
    name: &str,
    args: &Json,
    open: impl Fn(&str) -> Result<std::fs::File, Refusal>,
) -> Result<Item, Refusal> {
    let data;
    let items = match name {
        "write" => args["data"]
            .as_array()
            .ok_or_else(|| refused("TypeError", "write takes a list of ClipboardItem"))?,
        "writeText" => {
            let text = args["data"]
                .as_str()
                .ok_or_else(|| refused("TypeError", "writeText takes a string"))?;
            data = vec![json!({"text/plain": text})];
            &data
        }
        _ => return Err(refused("NotSupportedError", "unknown clipboard operation")),
    };
    let Some(first) = items.first() else {
        return Ok(Item::default());
    };
    if items.len() != 1 {
        return Err(refused(
            "NotAllowedError",
            "Support for multiple ClipboardItems is not implemented.",
        ));
    }
    let record = first
        .as_object()
        .filter(|r| !r.is_empty())
        .ok_or_else(|| refused("TypeError", "a ClipboardItem has at least one type"))?;
    if let Some(other) = record
        .keys()
        .find(|k| !matches!(k.as_str(), "text/plain" | "image/png"))
    {
        return Err((
            "NotAllowedError",
            format!("Type {other} not supported on write."),
        ));
    }
    let mut item = Item::default();
    if let Some(text) = record.get("text/plain") {
        let text = text
            .as_str()
            .ok_or_else(|| refused("TypeError", "text/plain is a string"))?;
        if text.len() > 1024 * 1024 {
            return Err(refused("DataError", "text/plain is over 1 MiB"));
        }
        item.text = Some(text.into());
    }
    if let Some(image) = record.get("image/png") {
        let path = image.as_str().ok_or_else(|| {
            if image.get("base64").and_then(Json::as_str).is_some() {
                refused(
                    "NotSupportedError",
                    "image/png on Windows takes an app:/ path; in-memory bytes are not supported",
                )
            } else {
                refused("TypeError", "image/png is an app:/ path")
            }
        })?;
        if !path.starts_with("app:/") {
            return Err(refused("TypeError", "image/png is an app:/ path"));
        }
        let file = open(path)?;
        let meta = file
            .metadata()
            .map_err(|e| ("NotReadableError", e.to_string()))?;
        if !meta.is_file() {
            return Err(refused(
                "NotReadableError",
                "image/png is not a regular file",
            ));
        }
        if meta.len() > MOST_FILE {
            return Err(refused("DataError", "image/png is over 64 MiB"));
        }
        let mut bytes = Vec::new();
        file.take(MOST_FILE + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| ("NotReadableError", e.to_string()))?;
        item.image = Some(Image::decode(&bytes).map_err(|e| ("DataError", e))?);
    }
    Ok(item)
}

fn open(path: &str) -> Result<std::fs::File, Refusal> {
    // Admission has checked the caller's fs.read grant. The host's opener
    // keeps the read below the app root, refusing symlinks and reparse points.
    exact_painted::picker::open(path).ok_or_else(|| {
        let missing = exact_painted::picker::resolve(path).is_some_and(|p| {
            std::fs::symlink_metadata(p).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        });
        refused(
            if missing {
                "NotFoundError"
            } else {
                "NotReadableError"
            },
            "image/png path could not be opened",
        )
    })
}

impl Package for Clipboard {
    fn new(context: Context) -> Self {
        Self { context, writes: 0 }
    }

    fn later(&mut self, name: &str, args: &Json, _preferred: Option<&Json>, reply: Reply) {
        let result = if self.context.origin() != "app" {
            Err(refused("NotAllowedError", "person-required"))
        } else {
            prepare(name, args, open)
        };
        let result = result.and_then(|item| {
            let text = item.text.is_some();
            let size = item.image.as_ref().map(Image::dimensions);
            let agent = self.context.agent() && std::env::var("EXACT_DEVICE").as_deref() != Ok("real");
            let shown = item.description(agent);
            exact_windows::clipboard::write(item, agent)
                .map_err(|e| ("UnknownError", e))?;
            if text || size.is_some() {
                self.writes += 1;
                self.context.changed(&format!("exact-state:{}", json!({
                    "extension": "clipboard", "state": { "writes": self.writes, "lastWrite": shown },
                })));
                if let Some((width, height)) = size {
                    self.context.log(format!("wrote image/png {width}×{height}{}", if text { " and text/plain" } else { "" }));
                } else {
                    self.context.log("wrote text/plain");
                }
            }
            Ok(())
        });
        match result {
            Ok(()) => reply.send(json!({})),
            Err((code, message)) => {
                self.context
                    .log(format!("write refused: {code} ({message})"));
                reply.error(code, Some(&message));
            }
        }
    }
}
