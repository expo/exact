// Embedded for a bound transition or layout transition. Mirrors motion/src/parse.rs and
// Transition::validate, including whole-list rejection, reading each word
// through motion-words.js. src/paint.rs supplies accepted property
// names and their kernel policy. No word of a transition keeps its case, so
// the text is folded first.
(() => {
  const properties = PROPERTIES, maxTransitions = MAX_TRANSITIONS;
  const { trim, split, easing, lower, time, num } = WORDS;
  return (v, css = true) => {
    const out = { held: false, stacks: false, css: null };
    if (v == null) return out;
    if (typeof v !== "string") return null;
    const text = lower(trim(v));
    if (!text || text === "unset") return out;
    out.held = true;
    if (text === "none") return out;
    const declarations = split(text, ",");
    if (declarations.length > maxTransitions) return null;
    let layout = null;
    for (const declaration of declarations) {
      const parts = split(trim(declaration), " ").filter(Boolean);
      if (!parts.length || parts.length > 4) return null;
      let property = null, timing = 0, curve = "ease";
      const times = [];
      for (const part of parts) {
        const seconds = time(part);
        if (Number.isFinite(seconds)) times.push(seconds);
        else if (Object.hasOwn(properties, part)) {
          if (property !== null) return null;
          property = part;
        } else {
          if (timing || !(timing = easing(part))) return null;
          curve = part;
        }
      }
      const [duration = 0, delay = 0] = times;
      if (times.length > 2 || duration < 0 || (timing === 2 && (duration !== 0 || delay < 0))) return null;
      out.stacks ||= property === null || properties[property];
      // Layout is outside Property::ALL. Only the last omitted property or `all` covers it.
      if (property === null || property === "all") layout = [duration, delay, curve];
    }
    if (css && layout) out.css = `${num(layout[0] * 1000)} ${num(layout[1] * 1000)} ${easing(layout[2], true)}`;
    return out;
  };
})()
