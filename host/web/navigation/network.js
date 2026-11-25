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
const hextets = (text) => {
  const [head, tail] = text.split("::"), h = head?.match(/[^:]+/g) ?? [], t = tail?.match(/[^:]+/g) ?? [];
  return [
    ...h,
    ...Array(tail === undefined ? 0 : 8 - h.length - t.length).fill("0"),
    ...t
  ].map((x) => parseInt(x, 16));
};
// The address a host writes: four octets (an IPv4-mapped IPv6 address by the ones it carries), eight hextets, or null for a name.
const address = (host) => {
  if (!host.startsWith("[")) {
    return host.match(/^(\d+)\.(\d+)\.(\d+)\.(\d+)$/)?.slice(1).map(Number) ?? null;
  }
  const s = hextets(host.slice(1, -1));
  return s.slice(0, 5).every((x) => x === 0) && s[5] === 65535 ? [
    s[6] >> 8,
    s[6] & 255,
    s[7] >> 8,
    s[7] & 255
  ] : s;
};
// The cloud instance metadata service, which no grant admits as local: 169.254.169.254 and EC2's fd00:ec2::254 (exact_grants::is_cloud_metadata).
export const isCloudMetadata = (host) => {
  const a = address(String(host).toLowerCase());
  return a?.length === 4 ? a.join(".") === "169.254.169.254" : a?.map((x) => x.toString(16)).join(":") === "fd00:ec2:0:0:0:0:0:254";
};
export const isLocalNetwork = (host, loopback) => {
  host = String(host).toLowerCase();
  if (isCloudMetadata(host)) {
    return false;
  }
  const a = address(host), name = host.replace(/\.$/, "");
  if (a?.length === 4) {
    return a[0] === 10 || a[0] === 172 && a[1] >= 16 && a[1] <= 31 || a[0] === 192 && a[1] === 168 || a[0] === 169 && a[1] === 254 || loopback && a[0] === 127;
  }
  if (a) {
    return (a[0] & 65024) === 64512 || (a[0] & 65472) === 65152 || loopback && a.slice(0, 7).every((x) => x === 0) && a[7] === 1;
  }
  return /.\.local$/.test(name) || loopback && name === "localhost";
};
export { address };
