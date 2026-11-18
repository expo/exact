//! `scrollIntoView`'s request and its outcome (LLP 1070.000): the command an
//! action states and the agent's `tap … into`.
use exact_kernel::ViewId;
use exact_plan::Value;

/// How a row aligns in a port along one axis, CSS's `ScrollLogicalPosition`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    /// Its start at the port's start (the web's `block` default).
    Start,
    /// Its middle at the port's middle.
    Center,
    /// Its end at the port's end.
    End,
    /// The least movement that shows it whole (the web's `inline` default).
    Nearest,
}
impl Align {
    /// CSS's name, or none.
    pub fn parse(name: &str) -> Option<Align> {
        Some(match name {
            "start" => Align::Start,
            "center" => Align::Center,
            "end" => Align::End,
            "nearest" => Align::Nearest,
            _ => return None,
        })
    }
}

/// One request, as the command or the agent's `tap … into` states it.
#[derive(Debug, Clone, PartialEq)]
pub struct IntoView {
    /// The virtualized list's `id`.
    pub list: String,
    /// The row's key.
    pub key: Value,
    /// Alignment in vertical ports.
    pub block: Align,
    /// Alignment in horizontal ports.
    pub inline: Align,
    /// For an inner list, its outer row's key.
    pub row: Option<Value>,
    /// The list by its view instead of its `id`: the agent's `tap <list>
    /// into <key>`, whose target is a mounted list.
    pub view: Option<ViewId>,
    /// `behavior="smooth"` (LLP 1070.000 §6.2): the host animates to the
    /// destination; the window is built there at once, as for `auto`.
    pub smooth: bool,
    /// Explicit pixels into the row wrapper; overrides main-axis alignment.
    pub offset: Option<f64>,
}

/// How a request ended, for `state`.
#[derive(Debug, Clone, PartialEq)]
pub enum IntoViewStatus {
    /// Aligned within half a pixel.
    Done,
    /// Still correcting.
    Pending,
    /// Its corrections (`CORRECTIONS`) did not land it.
    Unconverged,
    /// The reader scrolled, a newer request came, or the key left.
    Cancelled,
    /// Refused before anything moved.
    Refused(String),
}
impl IntoViewStatus {
    pub(crate) fn json(&self, out: &mut String) {
        let name = match self {
            IntoViewStatus::Done => "done",
            IntoViewStatus::Pending => "pending",
            IntoViewStatus::Unconverged => "unconverged",
            IntoViewStatus::Cancelled => "cancelled",
            IntoViewStatus::Refused(why) => {
                crate::json::quote(&format!("refused: {why}"), out);
                return;
            }
        };
        out.push('"');
        out.push_str(name);
        out.push('"');
    }
}

impl IntoView {
    /// The command's arguments as the compiler orders them: the list's id,
    /// the key, then `block`, `inline`, `behavior`, `row` and `offset`, each `none`
    /// where the author left the default.
    pub(crate) fn from_command(args: &[Value]) -> Result<IntoView, String> {
        let text = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_string);
        let align = |v: Option<&Value>, default| match text(v) {
            None => Ok(default),
            Some(name) => Align::parse(&name).ok_or(format!("no alignment {name}")),
        };
        let list = text(args.first()).ok_or("the list is named by its id")?;
        let key = args.get(1).cloned().ok_or("a key")?;
        let offset = match args.get(6) {
            None | Some(Value::Option(None) | Value::Unit) => None,
            Some(Value::Number(n)) if n.is_finite() => Some(*n),
            _ => return Err("offset must be a finite number of CSS pixels".into()),
        };
        Ok(IntoView {
            list,
            key,
            offset,
            block: align(args.get(2), Align::Start)?,
            inline: align(args.get(3), Align::Nearest)?,
            row: args
                .get(5)
                .filter(|v| !matches!(v, Value::Option(None) | Value::Unit))
                .cloned(),
            view: None,
            smooth: text(args.get(4)).as_deref() == Some("smooth"),
        })
    }
}
