// SpringConfig::easing's unit release, sample grid and thinning (motion/src/spring.rs).
(() => {
  // Shortest decimal that reads back to this f32, in the fixed notation
  // Easing::css and exact_web::css::num use. Percentages round a second time.
  const cssNumber = value => {
    const n = Math.fround(value);
    if (n === 0) return '0';
    if (Number.isInteger(n) && Math.abs(n) < 1e9) return String(n);
    let decimal;
    for (let precision = 1; precision <= 9; precision++) {
      const [mantissa, exponent] = n.toExponential(precision - 1).split('e');
      const digits = Math.round(Number(mantissa) * 10 ** (precision - 1));
      // A power of two has an asymmetric rounding interval: the nearest
      // decimal can miss it while the next decimal fits.
      decimal = [0, -1, 1].map(offset => Number(`${digits + offset}e${Number(exponent) - precision + 1}`))
        .find(candidate => Math.fround(candidate) === n);
      if (decimal !== undefined) break;
    }
    const [digits, exponent] = String(decimal).split('e');
    if (exponent === undefined) return digits;
    const negative = digits.startsWith('-'), mantissa = negative ? digits.slice(1) : digits;
    const point = (mantissa.includes('.') ? mantissa.indexOf('.') : mantissa.length) + Number(exponent);
    const whole = mantissa.replace('.', '');
    return (negative ? '-' : '') + (point <= 0 ? '0.' + '0'.repeat(-point) + whole
      : point >= whole.length ? whole + '0'.repeat(point - whole.length) : whole.slice(0, point) + '.' + whole.slice(point));
  };
  const coefficients = ([stiffness, damping, mass]) => {
    const alpha = damping / (2 * mass), omegaSquared = stiffness / mass;
    const discriminant = alpha * alpha - omegaSquared;
    const values = [alpha, omegaSquared, discriminant];
    return values.every(Number.isFinite) ? values : null;
  };
  const sample = ([alpha, omegaSquared, discriminant]) => {
    const epsilon = omegaSquared * 1e-12;
    const at = t => {
      const decay = Math.exp(-alpha * t);
      if (discriminant < -epsilon) {
        const omega = Math.sqrt(-discriminant), b = alpha / omega;
        const cosine = Math.cos(omega * t), sine = Math.sin(omega * t);
        if (Number.isFinite(b)) return [decay * (cosine + b * sine), decay * ((-alpha + b * omega) * cosine + (-alpha * b - omega) * sine)];
        const sineOverOmega = sine / omega;
        return [decay * (cosine + alpha * sineOverOmega), decay * (-omegaSquared * sineOverOmega)];
      }
      if (Math.abs(discriminant) <= epsilon) return [decay * (1 + alpha * t), decay * (alpha - alpha * (1 + alpha * t))];
      const root = Math.sqrt(discriminant), r1 = -alpha + root, r2 = -alpha - root;
      const c1 = -r2 / (r1 - r2), c2 = 1 - c1;
      const e1 = Math.exp(r1 * t), e2 = Math.exp(r2 * t);
      const result = [c1 * e1 + c2 * e2, c1 * r1 * e1 + c2 * r2 * e2];
      if (result.every(Number.isFinite)) return result;
      const slow = -omegaSquared / (alpha + root), exp = Math.exp(slow * t);
      const phi = -exp * Math.expm1((r2 - slow) * t) / (slow - r2);
      return [exp - slow * phi, slow * exp - slow * (slow * phi + e2)];
    };
    let count = 0;
    while (count < 2400) {
      const [x, velocity] = at(count / 240);
      if (!Number.isFinite(x) || !Number.isFinite(velocity)) return null;
      if (Math.abs(x) < .001 && Math.abs(velocity) < .001) break;
      count++;
    }
    const duration = count / 240, step = Math.max(1, Math.floor((count + 1) / Math.max(1, Math.ceil(duration * 60))));
    const stops = [];
    for (let i = 0; i < count; i += step) stops.push(`${cssNumber(1 - at(i / 240)[0])} ${cssNumber(Math.fround((i / 240) / duration) * 100)}%`);
    stops.push('1 100%');
    const easing = `linear(${stops.join(',')})`;
    return [duration, easing];
  };
  return { sample, coefficients, cssNumber };
})()
