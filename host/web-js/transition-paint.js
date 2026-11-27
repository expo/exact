// Embedded for a bound transition or layout transition. Mirrors motion/src/parse.rs and
// Transition::validate, including whole-list rejection, reading each word
// through motion-words.js. src/paint.rs supplies accepted property
// names and their kernel policy. No word of a transition keeps its case, so
// the text is folded first. A layout spring is written as css.rs
// `layout_transition_css` writes it: `spring` and its curve from rest, over the
// time a move of LAYOUT_EXTENT points settles in.
(() => {
  const properties = PROPERTIES, maxTransitions = MAX_TRANSITIONS;
  const { trim, split, easing, lower, time, num, springConfig } = WORDS;
  // (The sampler is read when a spring is lowered, only for the row's text: the bundle may define it after this)
  const lowered = curve => { const values = SAMPLER.coefficients(springConfig(curve)), at = values && SAMPLER.sample(values, LAYOUT_EXTENT); return at && [at[0], `spring ${at[1]}`]; };
  return (v, css = true, pressItem) => {
    // (`layout`: whether a declaration covers layout, which needs no lowering to know)
    const out = { held: false, stacks: false, layout: false, css: null };
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
      pressItem?.(property, duration, delay, curve, timing === 2 ? springConfig(curve) : null, easing);
      // Layout is outside Property::ALL. Only the last omitted property or `all` covers it.
      if (property === null || property === "all") layout = [duration, delay, curve, timing === 2];
    }
    out.layout = layout !== null;
    if (css && layout) {
      const [duration, delay, curve, spring] = layout, written = spring ? lowered(curve) : [duration, easing(curve, true)];
      if (!written) return null;
      out.css = `${num(written[0] * 1000)} ${num(delay * 1000)} ${written[1]}`;
    }
    return out;
  };
})()
