// The auth callback page's script (LLP 1069.006 D4), served at
// /.exact/auth/callback.js: hand this page's URL (the code, state and iss)
// back to window.opener first, when it remains, and then on a same-origin
// BroadcastChannel — the channel that survives a provider's
// Cross-Origin-Opener-Policy. A popup-local logout identity is read and
// removed before either send. Then the page drops the code from the address
// and closes. It never prints the URL or identity.
(() => {
  let identity = null;
  try {
    const stored = sessionStorage.getItem('exact.auth.return');
    sessionStorage.removeItem('exact.auth.return');
    if (typeof stored === 'string' && stored) identity = stored;
  } catch {}
  const message = { type: 'exact-auth', url: location.href, ...(identity ? { identity } : {}) };
  try { window.opener?.postMessage(message, location.origin); } catch {}
  try { const channel = new BroadcastChannel('exact-auth'); channel.postMessage(message); channel.close(); } catch {}
  history.replaceState(null, '', location.pathname);
  window.close();
})();
