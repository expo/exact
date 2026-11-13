// Embedded only for a bound transition. Mirrors motion/src/parse.rs and
// Transition::validate, including whole-list rejection, reading each word
// through motion-words.js. src/paint.rs supplies accepted property
// names and their kernel policy. No word of a transition keeps its case, so
// the text is folded first.
v => {
  const properties = PROPERTIES, maxTransitions = MAX_TRANSITIONS;
  const { trim, split, easing, lower, time } = WORDS;
  const text = lower(trim(String(v ?? "")));
  if (!text || text === "none") return false;
  const declarations = split(text, ",");
  if (declarations.length > maxTransitions) return false;
  let stacks = false;
  for (const declaration of declarations) {
    const parts = split(trim(declaration), " ").filter(Boolean);
    if (!parts.length || parts.length > 4) return false;
    let property = null, timing = 0;
    const times = [];
    for (const part of parts) {
      const seconds = time(part);
      if (Number.isFinite(seconds)) times.push(seconds);
      else if (Object.hasOwn(properties, part)) {
        if (property !== null) return false;
        property = part;
      } else {
        if (timing || !(timing = easing(part))) return false;
      }
    }
    const [duration = 0, delay = 0] = times;
    if (times.length > 2 || duration < 0 || (timing === 2 && (duration !== 0 || delay < 0))) return false;
    stacks ||= property === null || properties[property];
  }
  return stacks;
}
