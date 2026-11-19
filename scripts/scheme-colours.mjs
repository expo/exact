// The manifest's colours by colour scheme (LLP 1163 D2). `background_color` and `theme_color` at the top level hold in
// every scheme; under `color_scheme_light` or `color_scheme_dark` they hold in that scheme alone and win there. A
// scheme with neither is left to Exact: the launch colour is then the app's own ground where the build can read it,
// else the platform's page. A fixed `userInterfaceStyle` shows its side's colour in both. A colour is hex, or Display
// P3 as CSS writes it, `color(display-p3 r g b [/ a])`, which a host that shows only sRGB takes clipped.

/** Each scheme's colour for `member` (`background_color` or `theme_color`) as written, null where none is named.
 * `style` is the `userInterfaceStyle` the host honours: null on one that follows the device whatever it says. */
export function schemeColours(manifest, member, style = manifest.userInterfaceStyle) {
  const named = (scheme) => manifest[`color_scheme_${scheme}`]?.[member] ?? manifest[member] ?? null;
  if (style === 'light' || style === 'dark') {
    const one = named(style);
    return { light: one, dark: one };
  }
  return { light: named('light'), dark: named('dark') };
}

/** A manifest colour: its space (`srgb` or `display-p3`), its components from 0 to 1 in that space, its alpha, and its
 * sRGB form as `#rrggbb` (`#rrggbbaa` when translucent), clipped for a Display P3 colour outside sRGB. Throws naming
 * `field` for anything else. */
export function parseColour(text, field) {
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.exec(text ?? '');
  if (hex) {
    const digits = hex[1].length === 3 ? [...hex[1]].map((c) => c + c).join('') : hex[1];
    const byte = (i) => parseInt(digits.slice(i, i + 2), 16) / 255;
    const alpha = digits.length === 8 ? byte(6) : 1;
    return { space: 'srgb', rgb: [byte(0), byte(2), byte(4)], alpha, hex: srgbHex([byte(0), byte(2), byte(4)], alpha) };
  }
  const p3 = /^color\(\s*display-p3\s+(\S+)\s+(\S+)\s+([^\s/)]+)\s*(?:\/\s*([^\s)]+)\s*)?\)$/i.exec(text ?? '');
  const parts = p3?.slice(1, 4).map(unit), alpha = p3?.[4] === undefined ? 1 : unit(p3[4]);
  if (!parts || parts.includes(null) || alpha === null) {
    throw new Error(`${field} is ${JSON.stringify(text)}, which is not a colour app.json takes: write it as #RGB, #RRGGBB or #RRGGBBAA, or in Display P3 as color(display-p3 r g b) or color(display-p3 r g b / a), each from 0 to 1 or 0% to 100%`);
  }
  const linear = mul(LINEAR_P3_TO_SRGB, parts.map(toLinear));
  return { space: 'display-p3', rgb: parts, alpha, hex: srgbHex(linear.map(fromLinear), alpha) };
}

/** A component or alpha: a number from 0 to 1, or a percentage; null for anything else */
const unit = (s) => {
  const m = /^(\d*\.?\d+)(%?)$/.exec(s);
  const v = m && Number(m[1]) / (m[2] ? 100 : 1);
  return m && v <= 1 ? v : null;
};

const srgbHex = (rgb, alpha) => `#${[...rgb, ...(alpha < 1 ? [alpha] : [])].map((c) => Math.round(Math.min(Math.max(c, 0), 1) * 255).toString(16).padStart(2, '0')).join('')}`;
const toLinear = (c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const fromLinear = (c) => (c <= 0.0031308 ? c * 12.92 : 1.055 * c ** (1 / 2.4) - 0.055);
const mul = (m, v) => m.map((row) => row.reduce((sum, x, i) => sum + x * v[i], 0));
/** color/src/lib.rs `LINEAR_P3_TO_SRGB`, the same digits */
const LINEAR_P3_TO_SRGB = [[1.22494017628056, -0.22494017628055996, 0], [-0.04205695470968818, 1.0420569547096883, 0], [-0.01963755459033439, -0.0786360455506318, 1.0982736001409665]];
