// The settings a launch gives (LLP 1153.002 D2). A native bundle whose app.json marks a setting `"launch": true`
// calls `__exact_launch_settings` with those names as its first line, before any module's code
// (js/bake/src/typescript.mjs `launchIntro`); door 27 answers each with the launch's value, or nothing where
// the module takes the build's: at bake, on Android, for a name the module's grants do not carry
// (`env.launch`, js/src/launch.rs), in a realm with no door 27. app.ts's reads take
// `globalThis.__exact_launch` before the build's value. Asked once: the values are fixed for the launch.
(function (global) {
  "use strict";
  var host = global.__exact_host;
  var launched = null;
  global.__exact_launch_settings = function (names) {
    if (launched) throw new Error("the settings a launch gives are read once, as the module loads");
    launched = Object.create(null);
    for (var i = 0; i < names.length; i++) {
      var value;
      try { value = host(27, String(names[i]), ""); } catch (e) { value = undefined; }
      if (value) launched[names[i]] = value;
    }
    Object.defineProperty(global, "__exact_launch", { value: Object.freeze(launched), writable: false, configurable: false });
  };
  // A request the grants refuse at an origin a launch's setting gave names the setting (D3): the grants are the
  // build's, and a launch does not widen them. prelude.js's `__exact_fulfill` takes it.
  // The URL standard's origin: scheme, host and port, the scheme's default port left out, no credentials.
  function origin(url) {
    try { var at = new global.URL(String(url)).origin; return at && at !== "null" ? at : null; } catch (e) { return null; }
  }
  global.__exact_launch_refusal = function (failed, url) {
    var at = origin(url);
    if (!launched || !at || failed.kind !== "Refused" || failed.message !== "outside the app's grants (net.fetch)") return failed;
    for (var name in launched) {
      if (origin(launched[name]) === at) return { kind: failed.kind, message: failed.message + ": " + at + " is " + name + "'s value, which this launch gave, and grants are fixed at build: grant what a launch may name (net.fetch https:, net.fetch http:, or that origin)" };
    }
    return failed;
  };
})(globalThis);
