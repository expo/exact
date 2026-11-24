// @ref LLP 1077 D12 — the web's stand-ins for SF Symbol effects, played on an element whose own style has
// `--exact-symbol-effect` (css.rs writes it for bounce, pulse and scale). With `symbolEffectValue` (the
// element's `data-symboleffectvalue`), bounce and pulse play once each time the value changes, not for the value
// first shown; without it, they run while set. Scale has no discrete form on Apple's platforms: the symbol is
// held scaled up while set. Bounce and scale move the symbol, so `prefers-reduced-motion` leaves them out.
// Each animation composites over the element's own `scale` and `filter`.
// Timed against UIKit's on iOS 26: a pulse dims to about a third and back in 2 s (1.8 s a cycle while set); a
// bounce grows to 1.2, dips to 0.9 and settles in 0.6 s, and while set repeats every 1.5 s.
const PULSE = [{ filter: "opacity(1)", easing: "ease-in-out" }, { filter: "opacity(.3)", easing: "ease-in-out" }, { filter: "opacity(1)" }];
const BOUNCE = [{ scale: "1", easing: "ease-out" }, { scale: "1.2", offset: 0.35, easing: "ease-in-out" }, { scale: "0.9", offset: 0.65, easing: "ease-in-out" }, { scale: "1" }];
const ONCE = { bounce: { frames: BOUNCE, duration: 600 }, pulse: { frames: PULSE, duration: 2000 } };
// What runs while set, made at its first play, so a bundle that never plays an effect drops it
let Running = null;
const running = (key) => (Running ??= {
  bounce: { frames: [...BOUNCE.map((k, i) => ({ ...k, offset: [0, 0.35, 0.65, 1][i] * 0.4 })), { scale: "1" }], duration: 1500, iterations: Infinity },
  pulse: { frames: PULSE, duration: 1800, iterations: Infinity },
  scale: { frames: [{ scale: "1" }, { scale: "1.2" }], duration: 250, fill: "forwards" },
})[key];
const MOVES = new Set(["bounce", "scale"]);
let seen = false, installed = false;
// Each element's effect, the one it runs while set ("" for none), whether Reduce Motion stills it, the last value
// seen and the animation playing.
const shown = new Map();
// Reduce Motion, read once an effect is seen
let motion = null;
const animate = (el, { frames, duration, iterations = 1, fill = "none" }) => el.animate(frames, { duration, iterations, fill, composite: "add" });
const see = () => {
  seen = true;
  motion ??= globalThis.matchMedia?.("(prefers-reduced-motion: reduce)");
  motion?.addEventListener?.("change", playSymbolEffects);
};
/** A batch's style may name an effect: from then on, each tree update plays them. */
export const noteSymbolEffect = (css) => {
  if (!seen && css?.includes("--exact-symbol-effect")) {
    see();
  }
};
/** After each tree update: start or stop what runs while set, and play once what a changed value asks for. As on
 * Apple, a new effect ends the old one's play and waits for a change of the value. */
export function playSymbolEffects() {
  if (!seen || typeof document === "undefined" || globalThis.__exactRender) {
    return;
  }
  const now = new Set(document.querySelectorAll("[data-symbol-source][style*=\"--exact-symbol-effect\"]"));
  for (const [el, s] of shown) {
    if (!now.has(el)) {
      s.playing?.cancel();
      shown.delete(el);
    }
  }
  for (const el of now) {
    const effect = el.style.getPropertyValue("--exact-symbol-effect").trim(), value = el.getAttribute("data-symboleffectvalue");
    const key = value != null && ONCE[effect] ? "" : effect, still = MOVES.has(effect) && !!motion?.matches;
    let s = shown.get(el);
    if (!s || s.effect !== effect || s.key !== key || s.still !== still) {
      const same = s && s.effect === effect && s.key === key;
      s?.playing?.cancel();
      s = { effect, key, still, value: same ? s.value : value, playing: running(key) && !still ? animate(el, running(key)) : null };
      shown.set(el, s);
    }
    if (!key && value !== s.value && !still) {
      s.playing?.cancel();
      s.playing = animate(el, ONCE[effect]);
    }
    s.value = value;
  }
}
/** The JS target's: after each tree update (rt.js `After`), ahead of what the agent's clock registers there. */
export function installSymbolEffects(after) {
  if (!installed) {
    installed = true;
    see();
    after.unshift(playSymbolEffects);
  }
}
