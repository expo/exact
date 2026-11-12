// The settings a launch gives (LLP 1153.002 D2). A native bundle whose app.json marks a setting `"launch": true`
// begins with a launch module that calls `__exact_launch_settings` with those names before any other module's code
// (js/bake/src/typescript.mjs `launchModule`); door 27 answers each with the launch's value, or nothing where
// the module takes the build's: at bake, on Android, for a name the module's grants do not carry
// (`env.launch`, js/src/launch.rs), in a realm with no door 27. It answers a frozen object with no prototype, which
// app.ts's reads take. Asked once: the values are fixed for the launch.
(function (global) {
  "use strict";
  // Taken now, before any module's code, which could replace either global later.
  var host = global.__exact_host, NativeURL = global.URL;
  var launched = null;
  global.__exact_launch_settings = function (names) {
    if (launched) throw new Error("the settings a launch gives are read once, as the module loads");
    launched = Object.create(null);
    for (var i = 0; i < names.length; i++) {
      var value;
      try { value = host(27, String(names[i]), ""); } catch (e) { value = undefined; }
      if (value) launched[names[i]] = value;
    }
    return Object.freeze(launched);
  };
  // A request the grants refuse at an origin a launch's setting gave names the setting (D3): the grants are the
  // build's, and a launch does not widen them. prelude.js's `__exact_fulfill` takes it.
  // The URL standard's origin: scheme, host and port, the scheme's default port left out, no credentials.
  function origin(url) {
    try { var at = new NativeURL(String(url)).origin; return at && at !== "null" ? at : null; } catch (e) { return null; }
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
