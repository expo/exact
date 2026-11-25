import { matchesPage } from "./network.js";
// WHATWG URL normalization for the wasm grant parser, using the browser's
// existing tables. This is pure parsing: it grants no host I/O to the app.
export function grantOrigins(memory) {
  return { page_admits(ptr, len) {
    const bytes = new Uint8Array(memory().buffer, ptr >>> 0, len >>> 0);
    try { return Number(matchesPage(new URL(new TextDecoder().decode(bytes)))); } catch { return 0; }
  }, origin(ptr, len, wildcard, out, capacity) {
    try {
      const target = new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array(memory().buffer, ptr >>> 0, len >>> 0));
      const u = new URL(target), port = u.port || {
        "http:": 80,
        "https:": 443,
        "ws:": 80,
        "wss:": 443,
        "ftp:": 21
      }[u.protocol];
      if (!u.hostname || port == null) {
        return -1;
      }
      const address = u.hostname.startsWith("[") || /^(?:https?|wss?|ftp):$/.test(u.protocol) && /^[\d.]+$/.test(u.hostname);
      if (wildcard && (address || !["", "/"].includes(u.pathname) || target.includes("?") || target.includes("#") || u.hostname.endsWith(".") || u.hostname.split(".").filter(Boolean).length < 2)) {
        return -1;
      }
      const bytes = new TextEncoder().encode([
        u.protocol.slice(0, -1),
        u.hostname.toLowerCase(),
        port
      ].join("\0"));
      if (capacity) {
        if (capacity < bytes.length) {
          return -1;
        }
        new Uint8Array(memory().buffer, out >>> 0, bytes.length).set(bytes);
      }
      return bytes.length;
    } catch {
      return -1;
    }
  } };
}
