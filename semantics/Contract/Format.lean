/-
The roster's formatting and localization, as the runner computes them.

`formatTime` is `runner/src/stdlib.rs`'s, `formatDate`, `formatDay`, `formatNumber`,
`toFixed` and `formatDecimal` the linked `format` capability's (`runner/src/format/mod.rs`), `t` the
strings tables' (`exact_plan::strings`: `Plan::localized` and `fill`).
Each is transcribed: `en-US` at the fixed UTC offset the call names, the
calendar by Hinnant's `civil_from_days` over integers, compact numbers
from the shortest decimal digits Rust's `{}` prints (`shortestUp`: of two
equally near shortest forms, the upper, where JavaScript takes the even),
and decimal, currency and percent numbers from JavaScript's (`styled`); in
a table's locale, from the format data the plan carries for it (the last
section, LLP 1054.000.003 D6).
-/
import Contract.Number
import Contract.Value

namespace Contract.Format

open Number

/-- Rust's `{}` digits of a finite nonzero double: the shortest decimal
that reads back as `|x|`, the nearest of them, a tie the upper (core's
Dragon rounds up at exactly half). `Number.shortest` with that one change. -/
def shortestUp (p : Parts) : Nat × Int :=
  let x := absQ p
  let (lo, hi, incl) := roundingInterval p
  let inside (c : Q) : Bool :=
    if incl then lo.le c && c.le hi else lo.lt c && c.lt hi
  let dist (c : Q) : Q :=
    let a := c.num * x.den
    let b := x.num * c.den
    { num := if a ≥ b then a - b else b - a, den := c.den * x.den }
  let top := floorLog10 x
  let rec go (k : Nat) (q : Int) : Nat × Int :=
    match k with
    | 0 => (roundHalfEven (Q.mul x (pow10 (-q))), q)
    | k + 1 =>
      let s := roundHalfEven (Q.mul x (pow10 (-q)))
      let cands := [s, s + 1] ++ (if s > 0 then [s - 1] else [])
      let ok := cands.filter (fun c => c > 0 && inside (Q.mul { num := c, den := 1 } (pow10 q)))
      match ok with
      | [] => go k (q - 1)
      | c :: cs =>
        let best := cs.foldl (fun b c =>
          let db := dist (Q.mul { num := b, den := 1 } (pow10 q))
          let dc := dist (Q.mul { num := c, den := 1 } (pow10 q))
          if dc.lt db then c else if db.lt dc then b else max b c) c
        (best, q)
  let (s, q) := go 40 top
  let rec strip (fuel : Nat) (s : Nat) (q : Int) : Nat × Int :=
    match fuel with
    | 0 => (s, q)
    | f + 1 => if s % 10 == 0 && s > 0 then strip f (s / 10) (q + 1) else (s, q)
  strip 40 s q

def zeros (n : Nat) : List Char := List.replicate n '0'

/-- Rust's `{}` of `|x|` (finite, nonzero), never exponential, as its
integer digits (`[]` below one) and its fraction digits. -/
def rustDecimal (x : F64) : List Char × List Char :=
  let (s, q) := shortestUp (parts x)
  let ds := (toString s).toList
  let k : Int := ds.length
  let n := q + k
  if n ≤ 0 then ([], zeros (-n).toNat ++ ds)
  else if n < k then (ds.take n.toNat, ds.drop n.toNat)
  else (ds ++ zeros (n - k).toNat, [])

/-- An integer part grouped by threes. -/
def group (int : List Char) : List Char :=
  let len := int.length
  (int.zipIdx.map fun (d, i) => if i > 0 && (len - i) % 3 == 0 then [',', d] else [d]).flatten

/-- `formatNumber(n, "compact")`: `Intl.NumberFormat("en-US", { notation:
"compact", roundingMode: "trunc", signDisplay: "negative" })` as
`format.rs`'s `compact` reads the shortest digits: the suffix from the
unrounded magnitude (K, M, B, T, T above), every integer digit and below
ten two significant digits, truncated; grouping from five integer digits;
`""` for a non-finite value. -/
def compact (n : F64) : String :=
  if !isFinite n then ""
  else if n == 0 then "0"
  else
    let (int, frac) := rustDecimal n
    let scale := if int.length ≥ 4 then min ((int.length - 1) / 3) 4 else 0
    let cut := int.length - 3 * scale
    let moved := int.drop cut
    let int := int.take cut
    let frac := moved ++ frac
    let kept := match int.length with
      | 0 =>
        let first := (frac.findIdx? (· != '0')).getD frac.length
        frac.take (min (first + 2) frac.length)
      | 1 => frac.take (min 1 frac.length)
      | _ => []
    let kept := (kept.reverse.dropWhile (· == '0')).reverse
    let body :=
      (if int.isEmpty then ['0'] else if int.length ≥ 5 then group int else int) ++
      (if kept.isEmpty then [] else '.' :: kept)
    let suffix := match scale with
      | 1 => "K" | 2 => "M" | 3 => "B" | 4 => "T" | _ => ""
    (if n < 0 then "-" else "") ++ String.ofList body ++ suffix

/-- The ISO 4217 codes `formatNumber(n, "currency", code)` takes, each with
its `en-US` symbol and fraction digits (`format.rs`'s `CURRENCIES`). -/
def currencies : List (String × String × Nat) :=
  [("USD", "$", 2), ("EUR", "€", 2), ("GBP", "£", 2), ("JPY", "¥", 0), ("CNY", "CN¥", 2),
   ("INR", "₹", 2), ("CAD", "CA$", 2), ("AUD", "A$", 2), ("NZD", "NZ$", 2), ("HKD", "HK$", 2),
   ("SGD", "SGD", 2), ("CHF", "CHF", 2), ("SEK", "SEK", 2), ("NOK", "NOK", 2), ("DKK", "DKK", 2),
   ("PLN", "PLN", 2), ("MXN", "MX$", 2), ("BRL", "R$", 2), ("KRW", "₩", 0), ("ZAR", "ZAR", 2),
   ("TWD", "NT$", 2), ("ILS", "₪", 2), ("PHP", "₱", 2), ("VND", "₫", 0)]

/-- `format.rs`'s `rounded`: the integer and fraction digits of a finite
`|n|`, from JavaScript's shortest digits `s × 10^q` (`Number.shortest`, the
even of a tie, as ICU's are), the point moved `shift` places right, cut to
`hi` fraction digits half away from zero, trailing zeros dropped down to
`lo`; the integer part has no leading zero but a lone `0`. -/
def rounded (n : F64) (shift lo hi : Nat) : List Char × List Char :=
  -- `|n| × 10^(shift + hi)`, rounded half up: the kept digits.
  let kept : Nat :=
    if n == 0 then 0
    else
      let (s, q) := shortest (parts n)
      let e : Int := q + (shift + hi : Nat)
      if e ≥ 0 then s * 10 ^ e.toNat
      else let d := 10 ^ (-e).toNat; (s + d / 2) / d
  let ds := (toString kept).toList
  let ds := zeros (hi + 1 - ds.length) ++ ds
  let int := ds.take (ds.length - hi)
  let frac := ds.drop (ds.length - hi)
  let trimmed := (frac.reverse.dropWhile (· == '0')).reverse
  (int, frac.take (max trimmed.length lo))

/-- `formatNumber(n, "decimal" | "currency" | "percent")` (LLP 1116 D8):
`Intl.NumberFormat("en-US")`'s as `format.rs`'s `styled` computes it, from
JavaScript's shortest digits `s × 10^q` of `|n|` (`Number.shortest`, the
even of a tie, as ICU's are): the point moved `shift` places right (two for
`percent`), cut to `max` fraction digits half away from zero, trailing
zeros dropped down to `min` (`decimal` keeps at most three, `currency` its
code's exactly, `percent` none), the integer part grouped by threes, a
negative value (zero too) signed before the symbol, and a symbol that ends
in a letter set apart by a no-break space; `""` for a non-finite value.
`none` for a style or code the compiler would have refused. -/
def styled (n : F64) (style code : String) : Option String :=
  let spec : Option (String × Nat × Nat × Nat) :=
    if style = "decimal" then .some ("", 0, 3, 0)
    else if style = "percent" then .some ("", 0, 0, 2)
    else if style = "currency" then
      (currencies.find? (·.1 == code)).map fun (_, sym, d) => (sym, d, d, 0)
    else .none
  spec.map fun (sym, lo, hi, shift) =>
    if !isFinite n then ""
    else
      let (int, frac) := rounded n shift lo hi
      let sym := if sym.toList.getLast?.any Char.isAlpha then sym ++ "\u00A0" else sym
      (if signBit n then "-" else "") ++ sym ++ String.ofList (group int) ++
        (if frac.isEmpty then "" else "." ++ String.ofList frac) ++
        (if style = "percent" then "%" else "")

/-- 0001-01-01T00:00 and 9999-12-31T23:59:59.999, in ms since the epoch. -/
def firstWall : F64 := -(62135596800000 : F64)
def lastWall : F64 := 253402300799999

/-- `wall_ms`: the wall time of `epoch` at `offset` minutes east, or none
(a non-finite argument, an offset past ±18 h, a wall time outside years
1–9999). The instant is truncated toward zero first. -/
def wallMs (epoch offset : F64) : Option F64 :=
  if !isFinite epoch || !isFinite offset || offset > 1080 || offset < -(1080 : F64) then .none
  else
    let wall := trunc epoch + offset * 60000
    if firstWall ≤ wall && wall ≤ lastWall then .some wall else .none

/-- A strict day key's midnight, without shifting its calendar day.
The offset is still bounded as for timestamps, and years are 1–9999. -/
def dayKeyWall (key : String) (offset : F64) : Option F64 := do
  let _ ← wallMs 0 offset
  let (year, month, day) ← Str.isoDate key
  if year = 0 then .none
  else
    let year := year - (if month ≤ 2 then 1 else 0)
    let era := year / 400
    let yoe := year - era * 400
    let month := if month > 2 then month - 3 else month + 9
    let doy := (153 * month + 2) / 5 + day - 1
    let days := era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy
    return (F64.ofNat days - 719468) * 86400000

/-- Rust's `f64::rem_euclid`. -/
def remEuclid (a b : F64) : F64 :=
  let r := fmod a b
  if r < 0 then r + (if b < 0 then -b else b) else r

/-- A non-negative integral double as a `Nat` (Rust's `as u32` on one in range). -/
def toNat (x : F64) : Nat := x.toNat

/-- `x.floor() as i64` for `|x|` well inside `i64`. -/
def floorInt (x : F64) : Int := Int.ediv x.floor.scaled (2 ^ 1074)

/-- `formatTime(epochMs, utcOffset, "short")`: `h:mm AM`, `""` when invalid. -/
def formatTime (epoch offset : F64) : String :=
  match wallMs epoch offset with
  | .none => ""
  | .some wall =>
    let minutes := toNat ((remEuclid wall 86400000) / 60000).floor
    let h := minutes / 60
    let m := minutes % 60
    let (h12, suffix) :=
      if h == 0 then (12, " AM") else if h ≤ 11 then (h, " AM")
      else if h == 12 then (12, " PM") else (h - 12, " PM")
    toString h12 ++ ":" ++ toString (m / 10) ++ toString (m % 10) ++ suffix

/-- The proleptic Gregorian (year, month, day) of `days` since 1970-01-01. -/
def civil (days : Int) : Int × Nat × Nat :=
  let z := days + 719468
  let era := Int.ediv z 146097
  let doe := Int.emod z 146097
  let yoe := Int.ediv (doe - Int.ediv doe 1460 + Int.ediv doe 36524 - Int.ediv doe 146096) 365
  let doy := doe - (365 * yoe + Int.ediv yoe 4 - Int.ediv yoe 100)
  let mp := Int.ediv (5 * doy + 2) 153
  let day := doy - Int.ediv (153 * mp + 2) 5 + 1
  let month := if mp < 10 then mp + 3 else mp - 9
  (yoe + era * 400 + (if month ≤ 2 then 1 else 0), month.toNat, day.toNat)

def months : List String :=
  ["January", "February", "March", "April", "May", "June", "July", "August", "September",
   "October", "November", "December"]

/-- `formatDate(epochMs, utcOffset, style)`: `Sep 26, 2026` (`medium`) or
`September 2026` (`month-year`), `""` when invalid. -/
def formatDate (epoch offset : F64) (monthYear : Bool) : String :=
  match wallMs epoch offset with
  | .none => ""
  | .some wall =>
    let (year, month, day) := civil (floorInt (wall / 86400000))
    let name := months.getD (month - 1) ""
    let head := if monthYear then name else String.ofList (name.toList.take 3) ++ " " ++ toString day ++ ","
    head ++ " " ++ toString year

/-- `formatDate(epochMs, utcOffset, "iso")`: `YYYY-MM-DD`, the date part of
`toISOString` at that wall time (LLP 1102 §3.4), `""` when invalid. Years
are 1–9999 (`wallMs`), so four digits. -/
def formatIsoDate (epoch offset : F64) : String :=
  match wallMs epoch offset with
  | .none => ""
  | .some wall =>
    let (year, month, day) := civil (floorInt (wall / 86400000))
    let pad (n w : Nat) : String := let s := toString n; String.ofList (zeros (w - s.length)) ++ s
    pad year.toNat 4 ++ "-" ++ pad month 2 ++ "-" ++ pad day 2

def weekdays : List String :=
  ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"]

/-- `formatDay(thenMs, nowMs, utcOffset, mode)` (LLP 1116.020 D8): `t`'s day
counted from `n`'s in the offset's calendar days. `day` is `Today`,
`Yesterday`, `Tomorrow` or the medium date; `day-time` adds ` at ` and the
short time; `list` (any other mode: `Eval.formatting` passes only the three)
is the short time today, `Yesterday`, the weekday two to six days back,
else `M/D/YY`. `""` when either instant is invalid. -/
def formatDay (t n offset : F64) (mode : String) : String :=
  match wallMs t offset, wallMs n offset with
  | .some wall, .some now =>
    let days := floorInt (wall / 86400000)
    let apart := days - floorInt (now / 86400000)
    if mode == "list" then
      if apart == 0 then formatTime t offset
      else if apart == -1 then "Yesterday"
      else if -6 ≤ apart && apart ≤ -2 then weekdays.getD (Int.emod (days + 4) 7).toNat ""
      else
        let (year, month, day) := civil days
        let yy := (Int.emod year 100).toNat
        toString month ++ "/" ++ toString day ++ "/" ++ toString (yy / 10) ++ toString (yy % 10)
    else
      let label :=
        if apart == 0 then "Today" else if apart == -1 then "Yesterday"
        else if apart == 1 then "Tomorrow" else formatDate t offset false
      if mode == "day-time" then label ++ " at " ++ formatTime t offset else label
  | _, _ => ""

/-! ## Fixed decimals (LLP 1102 §3.2, decided (c)) -/

/-- The digits of `n / 10^places`: one before the point at least, `places`
after it, no point for none. -/
def places (n : Nat) (places : Nat) : String :=
  let ds := (toString n).toList
  let ds := zeros (places + 1 - ds.length) ++ ds
  let int := ds.take (ds.length - places)
  let frac := ds.drop (ds.length - places)
  String.ofList int ++ (if places = 0 then "" else "." ++ String.ofList frac)

/-- A digits argument the compiler admitted: a whole number from 0 to `max`. -/
def digitsOf (d : F64) (max : Nat) : Option Nat :=
  if isFinite d && d.mag % 2 ^ 1074 == 0 && !(d < 0) && d.toNat ≤ max then .some d.toNat else .none

/-- `toFixed(x, digits)`: JavaScript's `Number.prototype.toFixed` on the
exact value `|x| = mag / 2^1074`: `n = ⌊|x| × 10^digits + 1/2⌋`, the larger
of two equally near (a tie away from zero), signed for any `x < 0`; `|x| ≥
10^21` as `toString` prints it; `""` for a non-finite `x` (D7). -/
def toFixed (x : F64) (digits : Nat) : String :=
  if !isFinite x then ""
  else if x.mag ≥ 10 ^ 21 * 2 ^ 1074 then jsToString x
  else
    let n := (x.mag * 10 ^ digits + 2 ^ 1073) / 2 ^ 1074
    (if x < 0 then "-" else "") ++ places n digits

/-- `formatDecimal(units, digits)`: an integer `units` as a decimal with
`digits` places, exactly; `-0` is `0`; `""` for a count that is not an
integer or not finite. -/
def formatDecimal (x : F64) (digits : Nat) : String :=
  if !isFinite x || x.mag % 2 ^ 1074 != 0 then ""
  else (if x < 0 then "-" else "") ++ places (x.mag / 2 ^ 1074) digits

/-! ## Localized text -/

/-- The strings tables: (locale, (key, text)…), the base first. -/
abbrev Tables := List (String × List (String × String))

def lookupText (key : String) : List (String × String) → Option String
  | [] => .none
  | (k, t) :: rest => if k == key then .some t else lookupText key rest

/-- `Plan::localized`: `key`'s text in the table named `locale`, else in
the base. -/
def localized (tables : Tables) (locale key : String) : Option String :=
  match (tables.find? (·.1 == locale)).bind (lookupText key ·.2) with
  | .some t => .some t
  | .none => tables.head?.bind (lookupText key ·.2)

/-- A placeholder's value: the first name/value pair named `name`, the
pairs read two by two (`chunks_exact(2)`). -/
def pairValue (name : String) : List (Option String) → Option String
  | n :: v :: rest => if n == .some name then v else pairValue name rest
  | _ => .none

/-- `strings::fill` of a validated message: text as written, `\{`, `\}`
and `\\` as the brace or backslash, `{$name}` or `{name}` as the value its
pair gives (the placeholder's own spelling when none does). -/
def fill (value : String → Option String) : Nat → List Char → List Char → List Char
  | 0, _, acc => acc
  | _ + 1, [], acc => acc
  | f + 1, '\\' :: c :: rest, acc => fill value f rest (acc ++ [c])
  | f + 1, '{' :: rest, acc =>
    let inner := rest.takeWhile (· != '}')
    let after := (rest.dropWhile (· != '}')).drop 1
    let raw := '{' :: inner ++ ['}']
    let e := ((inner.dropWhile Char.isWhitespace).reverse.dropWhile Char.isWhitespace).reverse
    let name := match e with | '$' :: r => r | r => r
    fill value f after (acc ++ ((value (String.ofList name)).map String.toList).getD raw)
  | f + 1, c :: rest, acc => fill value f rest (acc ++ [c])

/-- `t(locale, key, pairs)`: the text `localized` finds, filled. -/
def text (tables : Tables) (locale key : String) (pairs : List (Option String)) : Option String :=
  (localized tables locale key).map fun t =>
    String.ofList (fill (pairValue · pairs) (t.length + 1) t.toList [])

/-! ## In a table's locale (LLP 1054.000.003 D6)

`format/locale.rs` transcribed: the formats of a table whose format data
the plan carries, a record of fields (`contract/lower/src/formats.rs`)
split at U+001E, a list's items at U+001F and an item's parts at U+001D. -/

/-- The record's fields. -/
abbrev Record := List String

def fieldOf (r : Record) (i : Nat) : String := r.getD i ""
def itemsOf (s : String) : List String := s.splitOn "\u001F"
def itemOf (r : Record) (i j : Nat) : String := (itemsOf (fieldOf r i)).getD j ""
def partsOf (s : String) : List String := s.splitOn "\u001D"

/-- ASCII digits in the locale's own (field 0, `""` for ASCII). -/
def native (r : Record) (s : List Char) : List Char :=
  let ds := (fieldOf r 0).toList
  if ds.isEmpty then s
  else s.map fun c => if '0' ≤ c && c ≤ '9' then ds.getD (c.toNat - '0'.toNat) c else c

/-- An integer's digits and fraction digits with the locale's symbols and
grouping: groups of the primary size, then the secondary, when the integer
has at least the primary size and `least` (at least the locale's) more. -/
def bodyIn (r : Record) (decimal group grouping : String) (int frac : List Char) (least : Nat) : String :=
  let g := grouping.toList
  let size (i : Nat) : Nat := match g[i]? with | .some c => c.toNat - '0'.toNat | .none => 3
  let primary := size 0
  let secondary := size 1
  let grouped := int.length ≥ primary + max least (size 2)
  let marked := (int.zipIdx.map fun ((d, i) : Char × Nat) =>
    let left := int.length - i
    if i > 0 && grouped && Nat.ble primary left && (left - primary) % secondary == 0 then ['\x00', d] else [d]).flatten
  let marked := marked ++ (if frac.isEmpty then [] else '\x01' :: frac)
  String.join ((native r marked).map fun c =>
    if c == '\x00' then group else if c == '\x01' then decimal else c.toString)

/-- `bodyIn` with the record's own symbols and grouping. -/
def body (r : Record) (int frac : List Char) (least : Nat) : String :=
  bodyIn r (fieldOf r 1) (fieldOf r 2) (fieldOf r 3) int frac least

/-- One of a pair of templates (positive, negative). -/
def signed (pair : String) (negative : Bool) : String :=
  ((pair.map fun c => if c == '\u001D' then '\u001F' else c).splitOn "\u001F").getD
    (if negative then 1 else 0) "{#}"

/-- `decimal`, `percent` and `currency`: `rounded`'s digits in the
locale's template, a percentage and an amount of money in their own symbols
and grouping; `none` for a style or code the compiler refuses. -/
def styledIn (r : Record) (n : F64) (style code : String) : Option String :=
  let own := (fieldOf r 1, fieldOf r 2, fieldOf r 3)
  let spec : Option (String × String × (String × String × String) × Nat × Nat × Nat) :=
    if style = "decimal" then .some (fieldOf r 4, "", own, 0, 3, 0)
    else if style = "percent" then
      match itemsOf (fieldOf r 5) with
      | [_, _, dec, grp, grouping] => .some (fieldOf r 5, "", (dec, grp, grouping), 0, 0, 2)
      | _ => .none
    else if style = "currency" then
      match currencies.findIdx? (·.1 == code), currencies.find? (·.1 == code) with
      | .some i, .some (_, _, d) =>
        match partsOf (itemOf r 7 i) with
        | [t, sym, dec, grp, grouping] => .some (itemOf r 6 t.toNat!, sym, (dec, grp, grouping), d, d, 0)
        | _ => .none
      | _, _ => .none
    else .none
  spec.map fun (pair, sym, (dec, grp, grouping), lo, hi, shift) =>
    if !isFinite n then ""
    else
      let (int, frac) := rounded n shift lo hi
      ((signed pair (signBit n)).replace "{#}" (bodyIn r dec grp grouping int frac 1)).replace "{¤}" sym

/-- A shown compact number's plural category letter (`x` when the record
lists none): an integer's by its value (above 199, by 100 plus its last two
digits), 1.0–9.9's by tenths. -/
def category (r : Record) (int kept : List Char) : Char :=
  let tables := itemsOf (fieldOf r 9)
  let whole := (tables.getD 0 "").toList
  let tenths := (tables.getD 1 "").toList
  let value (ds : List Char) : Nat := ds.foldl (fun v d => v * 10 + (d.toNat - '0'.toNat)) 0
  match kept with
  | d :: _ => tenths.getD (value (int.drop (int.length - 1)) * 10 + (d.toNat - '0'.toNat) - 10) 'x'
  | [] =>
    let last := value (int.drop (int.length - 3))
    whole.getD (if int.length ≤ 3 && last < 200 then last else 100 + last % 100) 'x'

/-- `compact`: `compact`'s rule with the locale's magnitudes, plural forms
and grouping of at least two digits more than a group. -/
def compactIn (r : Record) (n : F64) : String :=
  if !isFinite n then ""
  else
    let (int, frac) := if n == 0 then ([], []) else rustDecimal n
    let entries := itemsOf (fieldOf r 8)
    let m : Option Nat := if int.isEmpty then .none else .some (min (int.length - 1) (entries.length - 1))
    let ps := partsOf ((m.map (entries.getD · "")).getD "")
    let shown := (ps.head?.getD "").toNat?
    let (int, frac) := match shown, m with
      | .some z, .some m =>
        let cut := int.length - (m + 1 - z)
        (int.take cut, int.drop cut ++ frac)
      | _, _ => (int, frac)
    let kept := match int.length with
      | 0 =>
        let first := (frac.findIdx? (· != '0')).getD frac.length
        frac.take (min (first + 2) frac.length)
      | 1 => frac.take (min 1 frac.length)
      | _ => []
    let kept := (kept.reverse.dropWhile (· == '0')).reverse
    let int := if int.isEmpty then ['0'] else int
    let template := match shown with
      | .some _ =>
        let c := category r int kept
        match (ps.drop 2).find? (fun (p : String) => p.front == c) with
        | .some p => String.ofList (p.toList.drop 1)
        | .none => ps.getD 1 "{#}"
      | .none => "{#}"
    let positive := template.replace "{#}" (body r int kept 2)
    if n < 0 then (signed (fieldOf r 4) true).replace "{#}" positive else positive

/-- A number padded to `width` digits, in the locale's digits. -/
def padded (r : Record) (n width : Nat) : String :=
  let s := (toString n).toList
  String.ofList (native r (zeros (width - s.length) ++ s))

/-- The `{…}` fields of a wall time filled into a template. -/
def fillTemplate (r : Record) (wall : F64) (names periods : String) : Nat → List Char → String → String
  | 0, _, acc => acc
  | _ + 1, [], acc => acc
  | f + 1, '{' :: rest, acc =>
    if !rest.contains '}' then acc ++ String.ofList ('{' :: rest) else
    let name := String.ofList (rest.takeWhile (· != '}'))
    let after := (rest.dropWhile (· != '}')).drop 1
    let (year, month, day) := civil (floorInt (wall / 86400000))
    let minutes := toNat ((remEuclid wall 86400000) / 60000).floor
    let h := minutes / 60
    let w := name.length
    let value :=
      if name = "y" then padded r year.toNat 1
      else if name = "yy" then padded r (Int.emod year 100).toNat 2
      else if name = "M" ∨ name = "MM" then padded r month w
      else if name = "d" ∨ name = "dd" then padded r day w
      else if name = "h" ∨ name = "hh" then padded r ((h + 11) % 12 + 1) w
      else if name = "H" ∨ name = "HH" then padded r h w
      else if name = "K" ∨ name = "KK" then padded r (h % 12) w
      else if name = "k" ∨ name = "kk" then padded r ((h + 23) % 24 + 1) w
      else if name = "m" ∨ name = "mm" then padded r (minutes % 60) w
      else if name = "N" then (itemsOf names).getD (month - 1) ""
      else if name = "a" then
        ((itemsOf periods).filterMap fun p => match partsOf p with
          | [start, n] => (start.toNat?).bind fun start => if start ≤ minutes then .some n else .none
          | _ => .none).getLastD ""
      else "{" ++ name ++ "}"
    fillTemplate r wall names periods f after (acc ++ value)
  | f + 1, c :: rest, acc => fillTemplate r wall names periods f rest (acc.push c)

def dateIn (r : Record) (wall : F64) (template names : Nat) : String :=
  let t := fieldOf r template
  fillTemplate r wall (fieldOf r names) "" (t.length + 1) t.toList ""

/-- The short time, in the hour cycle the viewer forced, else the locale's. -/
def timeIn (r : Record) (wall : F64) (cycle : Option String) : String :=
  let (t, p) := if cycle == .some "h12" then (19, 20) else if cycle == .some "h23" then (21, 22) else (17, 18)
  let t := fieldOf r t
  fillTemplate r wall "" (fieldOf r p) (t.length + 1) t.toList ""

/-- `formatDay` with the locale's words, dates, weekdays and the medium
date's joiner with the short time. -/
def dayIn (r : Record) (cycle : Option String) (t n offset : F64) (mode : String) : String :=
  match wallMs t offset, wallMs n offset with
  | .some wall, .some now =>
    let days := floorInt (wall / 86400000)
    let apart := days - floorInt (now / 86400000)
    if mode == "list" then
      if apart == 0 then timeIn r wall cycle
      else if apart == -1 then itemOf r 24 1
      else if -6 ≤ apart && apart ≤ -2 then itemOf r 16 (Int.emod (days + 4) 7).toNat
      else dateIn r wall 14 15
    else
      let label :=
        if apart == 0 then itemOf r 24 0 else if apart == -1 then itemOf r 24 1
        else if apart == 1 then itemOf r 24 2 else dateIn r wall 10 11
      if mode == "day" then label
      else ((fieldOf r 23).replace "{T}" (timeIn r wall cycle)).replace "{D}" label
  | _, _ => ""

/-- Where a table carries its format data among the strings tables: a key
no table's text has (`contract lean` writes it there). -/
def formatKey : String := "\u001Eformat"

/-- The format locale's slot, as `contract lean` names it: a root state the
observation leaves out, as the runner's and the JS target's do. -/
def formatSlot : String := "#format"

/-- `hour_cycle`: the hour cycle a Unicode extension forces, `h12` or `h23`. -/
def hourCycle : List String → Option String
  | key :: value :: rest =>
    if key.toLower == "hc" then
      if value.toLower == "h12" then .some "h12" else if value.toLower == "h23" then .some "h23" else .none
    else hourCycle (value :: rest)
  | _ => .none

/-- The hour cycle a tag's Unicode extension forces (`en-US-u-hc-h23`). -/
def tagCycle (tag : String) : Option String :=
  hourCycle (("-u-".intercalate ((tag.splitOn "-u-").drop 1)).splitOn "-")

/-- `Plan::format_data`: the record of the table a format locale names
(`de`, `de h23`, `de-u-ca-gregory`: the table's whole name, then after a
space the forced hour cycle) and the hour cycle it forces: the one after the
space, else the name's own. -/
def recordOf (tables : Tables) (locale : String) : Option (Record × Option String) :=
  let pieces := locale.splitOn " "
  let table := pieces.headD ""
  let cycle := match pieces with
    | [t] => tagCycle t
    | _ =>
      let c := " ".intercalate (pieces.drop 1)
      if c == "h12" || c == "h23" then .some c else .none
  (tables.find? (·.1 == table)).bind fun (_, texts) =>
    (lookupText formatKey texts).bind fun record =>
      if record.isEmpty then .none else .some (record.splitOn "\u001E", cycle)

end Contract.Format
