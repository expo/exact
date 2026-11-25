// CSS spring coverage and timing for a bound transition entry, whose words arrive folded to lower case (as
// `motion/src/word.rs` folds them), so the patterns here match lower case only.
entry => {
  const properties = PROPERTIES, all = ALL_PROPERTIES;
  const number = s => /^[+-]?(?:\d+(?:\.\d+)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(s) ? Number(s) : NaN;
  const parts = entry.split(/ (?![^(]*\))/).filter(Boolean);
  let property = 'all', named = false, config;
  const times = [];
  if (parts.length > 4) return [];
  for (const part of parts) {
    const spring = /^-exact-spring\((.*)\)$/.exec(part);
    if (spring && !config) config = spring[1].trim() ? spring[1].split(',').map(s => number(s.trim())) : [100, 10, 1];
    else if (Object.hasOwn(properties, part) && !named) { property = part; named = true; }
    else {
      const time = part.endsWith('ms') ? number(part.slice(0, -2)) * .001 : part.endsWith('s') ? number(part.slice(0, -1)) : NaN;
      if (!Number.isFinite(time)) return [];
      times.push(time);
    }
  }
  if (!config || config.length !== 3 || !config.every(Number.isFinite) || times.length > 2 || (times[0] ?? 0) !== 0 || (times[1] ?? 0) < 0) return [];
  if (!(config[0] > 0 && config[1] >= 0 && config[2] > 0)) return [];
  const names = property === 'all' ? all : properties[property];
  // Keep transition-property coverage while WAAPI plays physics springs.
  const coverage = `${property} 0s ease 0s`;
  const { sample, coefficients, cssNumber } = SAMPLER;
  const values = coefficients(config);
  if (!values) return [];
  if (!names.length) return [coverage];
  const timing = sample(values);
  if (!timing) return [];
  const [duration, easing] = timing;
  const curves = names.map(name => `${name} ${cssNumber(duration)}s ${easing} ${cssNumber(times[1] ?? 0)}s`);
  return property === 'all' ? [coverage, ...curves] : curves;
}
