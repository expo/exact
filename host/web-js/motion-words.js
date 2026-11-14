// The words of a bound transition or animation, as motion/src/word.rs and the
// grammars over it read them: a keyword, a unit and a function's name in any
// ASCII case, a number CSS's (not `inf`, `nan` or `1.`), an easing as
// Easing/SpringConfig::validate accept it. Embedded by transition-paint.js and
// animation-paint.js; src/paint.rs supplies the most `linear()` stops.
(() => {
  const maxStops = MAX_LINEAR_STOPS;
  const trim = s => s.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, "");
  const number = s => /^[+-]?(?:\d+(?:\.\d+)?|\.\d+)(?:[eE][+-]?\d+)?(?![\s\S])/.test(s) ? Number(s) : NaN;
  const split = (s, sep) => {
    const out = []; let depth = 0, start = 0;
    for (let i = 0; i < s.length; i++) {
      if (s[i] === "(") depth++;
      else if (s[i] === ")") depth--;
      else if (s[i] === sep && depth === 0) { out.push(s.slice(start, i)); start = i + 1; }
    }
    out.push(s.slice(start)); return out;
  };
  const springValid = ([stiffness, damping, mass]) => {
    if (!(stiffness > 0 && damping >= 0 && mass > 0)) return false;
    const alpha = damping / (2 * mass), omegaSquared = stiffness / mass;
    const discriminant = alpha * alpha - omegaSquared;
    if (![alpha, omegaSquared, discriminant].every(Number.isFinite)) return false;
    for (const elapsed of [0, 1 / 240, 10]) for (const velocity of [3.4028234663852886e38, -3.4028234663852886e38]) {
      const displacement = 3.4028234663852886e38;
      const epsilon = omegaSquared * 1e-12, decay = Math.exp(-alpha * elapsed);
      let x, speed;
      if (discriminant < -epsilon) {
        const omegaD = Math.sqrt(-discriminant), b = (velocity + alpha * displacement) / omegaD;
        const cosine = Math.cos(omegaD * elapsed), sine = Math.sin(omegaD * elapsed);
        if (Number.isFinite(b)) {
          x = decay * (displacement * cosine + b * sine);
          speed = decay * ((-alpha * displacement + b * omegaD) * cosine + (-alpha * b - displacement * omegaD) * sine);
        } else {
          const sineOverOmega = sine / omegaD;
          x = decay * (displacement * cosine + velocity * sineOverOmega + alpha * displacement * sineOverOmega);
          speed = decay * (velocity * cosine - alpha * velocity * sineOverOmega - omegaSquared * displacement * sineOverOmega);
        }
      } else if (Math.abs(discriminant) <= epsilon) {
        const b = velocity + alpha * displacement;
        x = decay * (displacement + b * elapsed);
        speed = decay * (b - alpha * (displacement + b * elapsed));
      } else {
        const root = Math.sqrt(discriminant), r1 = -alpha + root, r2 = -alpha - root;
        const c1 = (velocity - r2 * displacement) / (r1 - r2), c2 = displacement - c1;
        const e1 = Math.exp(r1 * elapsed), e2 = Math.exp(r2 * elapsed);
        x = c1 * e1 + c2 * e2;
        speed = c1 * r1 * e1 + c2 * r2 * e2;
      }
      if (!Number.isFinite(x) || !Number.isFinite(speed)) return false;
    }
    return true;
  };
  // css.rs `num` and Easing::css carry numbers as f32, in their shortest decimal.
  const num = n => {
    const f = Math.fround(n);
    if (!Number.isFinite(f)) return f === Infinity ? "inf" : f === -Infinity ? "-inf" : "NaN";
    if (Number.isInteger(f) && Math.abs(f) < 1e9) return String(f);
    for (let p = 1; p < 10; p++) {
      const value = Number(f.toPrecision(p));
      if (Math.fround(value) !== f) continue;
      const [mantissa, exponent] = String(value).split("e");
      if (exponent === undefined) return mantissa;
      const sign = value < 0 ? "-" : "", unsigned = mantissa.replace("-", "");
      const digits = unsigned.replace(".", ""), point = (unsigned.split(".")[0].length) + Number(exponent);
      return sign + (point <= 0 ? "0." + "0".repeat(-point) + digits
        : point >= digits.length ? digits + "0".repeat(point - digits.length)
        : digits.slice(0, point) + "." + digits.slice(point));
    }
    return String(f);
  };
  const easing = (s, css = false) => {
    if (["linear", "ease", "ease-in", "ease-out", "ease-in-out", "step-start", "step-end"].includes(s)) {
      return css ? s === "step-start" ? "steps(1,jump-start)" : s === "step-end" ? "steps(1,jump-end)" : s : 1;
    }
    const open = s.indexOf("(");
    if (open < 0 || !s.endsWith(")")) return 0;
    const name = s.slice(0, open), inner = trim(s.slice(open + 1, -1));
    const args = inner ? split(inner, ",").map(trim) : [];
    if (name === "steps") {
      const count = /^\+?\d+$/.test(args[0] ?? "") ? Number(args[0]) : 0;
      const position = args[1] ?? "jump-end";
      // A count and at most one position (CSS Easing 1 §3.1).
      if (!(args.length <= 2 && count > 0 && count <= 65535 && (position !== "jump-none" || count > 1)
        && ["jump-start", "start", "jump-end", "end", "jump-none", "jump-both"].includes(position))) return 0;
      return css ? `steps(${count},${position === "start" ? "jump-start" : position === "end" ? "jump-end" : position})` : 1;
    }
    if (name === "linear") {
      // As motion/src/parse.rs's linear_stops: a position may be infinite
      // until CSS Easing 2's fixup has run, and only then is it validated.
      const stops = [];
      for (const arg of args) {
        const fields = trim(arg).split(/\p{White_Space}+/u);
        const output = number(fields[0]);
        if (fields.length > 3 || Number.isNaN(output)) return 0;
        if (fields.length === 1) stops.push([null, output]);
        for (const field of fields.slice(1)) {
          const input = field.endsWith("%") ? number(field.slice(0, -1)) / 100 : NaN;
          if (Number.isNaN(input)) return 0;
          stops.push([input, output]);
        }
      }
      if (!stops.length) return 0;
      // The fixup: default the endpoints, clamp each position to the greatest
      // before it, then spread each run of omitted positions evenly.
      stops[0][0] ??= 0; stops[stops.length - 1][0] ??= 1;
      let greatest = -Infinity;
      for (const stop of stops) {
        if (stop[0] === null) continue;
        if (stop[0] < greatest) stop[0] = greatest;
        greatest = stop[0];
      }
      for (let start = 0; start + 1 < stops.length;) {
        if (stops[start + 1][0] !== null) { start++; continue; }
        let end = start + 2;
        while (stops[end][0] === null) end++;
        const from = stops[start][0], to = stops[end][0], width = end - start;
        for (let i = start + 1; i < end; i++) stops[i][0] = from + (to - from) * (i - start) / width;
        start = end;
      }
      // Easing::validate.
      if (stops.length < 2 || stops.length > maxStops) return 0;
      let last = 0;
      for (const [input, output] of stops) {
        if (!Number.isFinite(input) || !Number.isFinite(output) || input < 0 || input > 1 || input < last) return 0;
        last = input;
      }
      return css ? `linear(${stops.map(([input, output]) => `${num(output)} ${num(input * 100)}%`).join(",")})` : 1;
    }
    const n = args.map(number);
    if (!n.every(Number.isFinite)) return 0;
    if (name === "cubic-bezier") {
      if (!(n.length === 4 && n[0] >= 0 && n[0] <= 1 && n[2] >= 0 && n[2] <= 1)) return 0;
      return css ? `cubic-bezier(${n.map(num).join(",")})` : 1;
    }
    if (name === "-exact-spring") {
      if (!(n.length === 0 || (n.length === 3 && springValid(n)))) return 0;
      return css ? `spring(${(n.length ? n : [100, 10, 1]).map(num).join(", ")})` : 2;
    }
    return 0;
  };
  const lower = s => s.replace(/[A-Z]+/g, c => c.toLowerCase());
  // `word` without its unit `unit`, in any ASCII case; null when it has none.
  const unit = (word, u) => word.length >= u.length && lower(word.slice(-u.length)) === u ? word.slice(0, -u.length) : null;
  // A time's seconds (Infinity past a double, as Rust reads it); NaN when it is none.
  const time = word => {
    const ms = unit(word, "ms");
    if (ms !== null) return number(ms) * .001;
    const s = unit(word, "s");
    return s === null ? NaN : number(s);
  };
  // Written as a time, even one `time` refuses (`1.2.3s`).
  const timeLike = word => /^[+-]?\.?\d/.test(unit(word, "ms") ?? unit(word, "s") ?? "");
  return { trim, number, split, easing, lower, time, timeLike, num };
})()
