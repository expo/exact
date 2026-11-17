// Embedded only for a bound animation. Whether the value plays keyframes
// that stack (src/paint.rs names them), read as motion/src/animation/parse.rs
// reads the shorthand: each animation's words, through motion-words.js,
// take in turn the first free part they can be (its `Slots`: a time, an
// easing, a count, a direction, a fill mode, a play state, then the name, which
// keeps its case), and a value the grammar or Animation::validate refuses plays
// nothing.
v => {
  if (v == null) return false;
  const { trim, number, split, easing, lower, time, timeLike } = WORDS;
  const names = NAMES, maxAnimations = MAX_ANIMATIONS;
  const easingWord = w => ["linear", "ease", "ease-in", "ease-out", "ease-in-out", "step-start", "step-end"].includes(w)
    || ["cubic-bezier", "steps", "linear", "-exact-spring"].includes(w.split("(")[0]) && w.includes("(")
    || w.startsWith("spring(");
  const name = part => {
    if (/^(["']).*\1$/s.test(part) && part.length >= 2) return true;
    return /^(?:[A-Za-z_]|-(?![0-9]))[A-Za-z0-9_-]*$/.test(part) && part !== "-"
      && !["initial", "inherit", "unset", "revert", "revert-layer", "default"].includes(lower(part));
  };
  const text = trim(String(v));
  if (!text || lower(text) === "none") return false;
  let played = 0, stacks = false;
  for (const declaration of split(text, ",")) {
    const parts = split(trim(declaration), " ").filter(Boolean);
    if (!parts.length || parts.length > 8) return false;
    let times = 0, eased = false, counted = false, directed = false, filled = false, stated = false, named = null;
    let duration = 0, delay = 0;
    for (const part of parts) {
      const word = lower(part), seconds = time(part);
      if (!Number.isNaN(seconds)) {
        if (++times > 2) return false;
        if (times === 1) duration = seconds; else delay = seconds;
      } else if (timeLike(part)) return false;
      else if (!eased && easingWord(word)) {
        if (easing(word) !== 1) return false;
        eased = true;
      } else if (!counted && (word === "infinite" || number(part) >= 0)) counted = true;
      else if (!directed && ["normal", "reverse", "alternate", "alternate-reverse"].includes(word)) directed = true;
      else if (!filled && ["none", "forwards", "backwards", "both"].includes(word)) filled = true;
      else if (!stated && ["running", "paused"].includes(word)) stated = true;
      else if (named === null && name(part)) named = word === "none" ? "none" : part.replace(/^["']+|["']+$/g, "");
      else return false;
    }
    if (!Number.isFinite(duration) || !Number.isFinite(delay) || duration < 0) return false;
    if (named !== null && named !== "none") {
      played++;
      stacks ||= names.includes(named);
    }
  }
  return played <= maxAnimations && stacks;
}
