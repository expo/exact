// SHA-256 also works on trusted-LAN HTTP pages where SubtleCrypto is absent.
// The constants are the fractional square/cube roots of the first primes.
// All integer additions below are reduced modulo 2^32.
let hashConstants;
export async function digest(bytes) {
  if (globalThis.crypto?.subtle) return Array.from(new Uint8Array(await crypto.subtle.digest("SHA-256", bytes)), (n) => n.toString(16).padStart(2, "0")).join("");
  if (!hashConstants) {
    const primes = [];
    for (let n = 2; primes.length < 64; n++) if (primes.every((p) => n % p)) primes.push(n);
    hashConstants = { initial: primes.slice(0, 8).map((n) => (Math.sqrt(n) % 1 * 0x100000000) | 0),
      round: primes.map((n) => (Math.cbrt(n) % 1 * 0x100000000) | 0) };
  }
  const hash = hashConstants.initial.slice(), words = new Int32Array(64);
  const padded = new Uint8Array(Math.ceil((bytes.length + 9) / 64) * 64);
  padded.set(bytes); padded[bytes.length] = 128;
  const view = new DataView(padded.buffer);
  view.setUint32(padded.length - 8, Math.floor(bytes.length / 0x20000000));
  view.setUint32(padded.length - 4, bytes.length * 8);
  const rotate = (n, by) => n >>> by | n << (32 - by);
  for (let start = 0; start < padded.length; start += 64) {
    for (let i = 0; i < 16; i++) words[i] = view.getInt32(start + i * 4);
    for (let i = 16; i < 64; i++) {
      const a = words[i - 15], b = words[i - 2];
      words[i] = words[i - 16] + (rotate(a, 7) ^ rotate(a, 18) ^ a >>> 3) + words[i - 7] + (rotate(b, 17) ^ rotate(b, 19) ^ b >>> 10);
    }
    let [a, b, c, d, e, f, g, h] = hash;
    for (let i = 0; i < 64; i++) {
      const t1 = (h + (rotate(e, 6) ^ rotate(e, 11) ^ rotate(e, 25)) + (e & f ^ ~e & g) + hashConstants.round[i] + words[i]) | 0;
      const t2 = ((rotate(a, 2) ^ rotate(a, 13) ^ rotate(a, 22)) + (a & b ^ a & c ^ b & c)) | 0;
      h = g; g = f; f = e; e = d + t1 | 0; d = c; c = b; b = a; a = t1 + t2 | 0;
    }
    [a, b, c, d, e, f, g, h].forEach((n, i) => { hash[i] = hash[i] + n | 0; });
  }
  return hash.map((n) => (n >>> 0).toString(16).padStart(8, "0")).join("");
}
