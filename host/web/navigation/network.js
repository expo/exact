let documentOrigin;
function browserOrigin() {
  if (!globalThis.document) return undefined;
  if (documentOrigin === undefined) {
    // Blob URLs carry the creator's effective origin, including an opaque sandbox origin.
    const url = URL.createObjectURL(new Blob());
    try { documentOrigin = new URL(url).origin; } finally { URL.revokeObjectURL(url); }
  }
  return documentOrigin;
}
// Only the server bundle links the renderer's predicate. A browser module always uses its document.
const renderAdmitsPage = import.meta.exactPageAdmits;
export const hasPageOrigin = () => renderAdmitsPage ? renderAdmitsPage() : /^https?:\/\//.test(browserOrigin() ?? '');
export const matchesPage = url => (url.protocol === "http:" || url.protocol === "https:") &&
  (renderAdmitsPage ? renderAdmitsPage(url.origin) : url.origin === browserOrigin());

// a host on the local network as a URL writes it (LLP 1133.036 D5); with `loopback`, loopback too.
const ipv4 = (text) => {
  const parts = text.split(".");
  if (parts.length !== 4) return null;
  const octets = [];
  for (const part of parts) {
    if (!/^(?:0|[1-9]\d{0,2})$/.test(part)) return null;
    const n = Number(part);
    if (n > 255) return null;
    octets.push(n);
  }
  return octets;
};
const hextets = (text) => {
  if (text.includes(".")) {
    const colon = text.lastIndexOf(":");
    if (colon < 0) return null;
    const octets = ipv4(text.slice(colon + 1));
    if (!octets) return null;
    text = `${text.slice(0, colon + 1)}${((octets[0] << 8) | octets[1]).toString(16)}:${((octets[2] << 8) | octets[3]).toString(16)}`;
  }
  const halves = text.split("::");
  if (halves.length > 2) return null;
  const h = halves[0] === "" ? [] : halves[0].split(":");
  const t = halves.length === 2 ? (halves[1] === "" ? [] : halves[1].split(":")) : null;
  if (t === null ? h.length !== 8 : h.length + t.length >= 8) return null;
  const parts = t === null ? h : [...h, ...Array(8 - h.length - t.length).fill("0"), ...t];
  if (!parts.every((x) => /^[0-9a-f]{1,4}$/.test(x))) return null;
  return parts.map((x) => parseInt(x, 16));
};
// The address a host writes: four octets (an IPv4-mapped IPv6 address by the ones it carries), eight hextets, or null for a name.
const address = (host) => {
  if (typeof host !== "string") return null;
  host = host.toLowerCase();
  if (!host.startsWith("[")) {
    return ipv4(host);
  }
  if (!host.endsWith("]")) return null;
  const s = hextets(host.slice(1, -1));
  if (!s) return null;
  return s.slice(0, 5).every((x) => x === 0) && s[5] === 65535 ? [
    s[6] >> 8,
    s[6] & 255,
    s[7] >> 8,
    s[7] & 255
  ] : s;
};
// Whether `host`, as a URL writes it, is a loopback host (LLP 1016 D6): IPv4 127.0.0.0/8, IPv6 [::1],
// an IPv4-mapped IPv6 address carrying 127.0.0.0/8, or exact `localhost` (at most one trailing dot).
export const isLoopback = (host) => {
  if (typeof host !== "string") return false;
  host = host.toLowerCase();
  const a = address(host);
  if (a?.length === 4) {
    return a[0] === 127;
  }
  if (a) {
    return a.slice(0, 7).every((x) => x === 0) && a[7] === 1;
  }
  return host.replace(/\.$/, "") === "localhost";
};
// The cloud instance metadata service, which no grant admits as local: 169.254.169.254 and EC2's fd00:ec2::254 (exact_grants::is_cloud_metadata).
export const isCloudMetadata = (host) => {
  if (typeof host !== "string") return false;
  const a = address(host.toLowerCase());
  return a?.length === 4 ? a.join(".") === "169.254.169.254" : a?.map((x) => x.toString(16)).join(":") === "fd00:ec2:0:0:0:0:0:254";
};
export const isLocalNetwork = (host, loopback) => {
  if (typeof host !== "string") return false;
  if (loopback && isLoopback(host)) {
    return true;
  }
  host = host.toLowerCase();
  if (isCloudMetadata(host)) {
    return false;
  }
  const a = address(host), name = host.replace(/\.$/, "");
  if (a?.length === 4) {
    return a[0] === 10 || a[0] === 172 && a[1] >= 16 && a[1] <= 31 || a[0] === 192 && a[1] === 168 || a[0] === 169 && a[1] === 254;
  }
  if (a) {
    return (a[0] & 65024) === 64512 || (a[0] & 65472) === 65152;
  }
  return /.\.local$/.test(name);
};
export { address };
