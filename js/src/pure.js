// Ibex owns URL, URLSearchParams, Headers, and text. Preserve Exact's base64
// edge behavior.
(function (global) {
  "use strict";
  var host = global.__exact_host;
  function call(op, args) { return JSON.parse(host(7, op, JSON.stringify(args))); }
  function invalid(message) {
    var error = new Error(message);
    Object.defineProperty(error, "name", { value: "InvalidCharacterError" });
    return error;
  }
  global.btoa = function (input) {
    var text = String(input);
    for (var i = 0; i < text.length; i++) {
      if (text.charCodeAt(i) > 255) throw invalid("btoa requires Latin-1");
    }
    return call("btoa", [text]);
  };
  global.atob = function (input) {
    var text = String(input).replace(/[\t\n\f\r ]/g, "");
    if (text.length % 4 === 0) text = text.replace(/==?$/, "");
    if (text.length % 4 === 1 || /[^A-Za-z0-9+/]/.test(text)) throw invalid("invalid base64");
    var alphabet = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    var tail = text.length % 4;
    if (tail) {
      var value = alphabet.indexOf(text[text.length - 1]);
      text = text.slice(0, -1) + alphabet[value & (tail === 2 ? 48 : 60)];
      while (text.length % 4) text += "=";
    }
    return call("atob", [text]);
  };
})(globalThis);
