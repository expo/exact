// Installed before Ibex's bindings, which may capture `console`.
// @ref LLP 1027 D10 — console writes to the runner
// @ref LLP 1027.009 D1 — each line with the method that wrote it; the runtime's own with none
(function (global) {
  'use strict';
  var writeLog = global.__exact_log;
  delete global.__exact_log;
  if (writeLog) {
    var NativeError = global.Error, stringify = JSON.stringify, string = String;
    // JavaScript catches recover formatting errors while Hermes's uncatchable
    // execution interrupts propagate. A JSI catch would swallow both.
    function render(value) {
      try {
        if (value !== null && (typeof value === 'object' || typeof value === 'function')) {
          if (value instanceof NativeError) {
            var text = string(value);
            try {
              var stack = value.stack;
              if (typeof stack === 'string') {
                if (stack.indexOf(text) === 0) return stack;
                if (stack) text += '\n' + stack;
              }
            } catch (_) { /* Keep the name and message if the stack getter throws. */ }
            return text;
          }
          var json = stringify(value);
          if (typeof json === 'string') return json;
        }
      } catch (_) { /* Cycles, BigInts and getters can refuse serialization. */ }
      try { return string(value); }
      catch (_) { return '[unprintable]'; }
    }
    function method(name) {
      return function () {
        var line = '';
        for (var i = 0; i < arguments.length; i++) {
          if (i) line += ' ';
          line += render(arguments[i]);
        }
        writeLog(line, name);
      };
    }
    global.console = {
      log: method('log'), info: method('info'), warn: method('warn'),
      error: method('error'), debug: method('debug'),
    };
    // What the runtime itself says: journaled as `console: …`, as the app's
    // lines are, but with no method, so no host writes it to the platform's
    // log. The prelude's files take it, and intl.js deletes it before the app
    // loads: it is not the app's to call.
    global.__exact_runtime_line = function (line) { writeLog(string(line), ''); };
    // An error event no listener cancelled, as a browser's console prints
    // it: written to the platform's log alone, at error level, as the
    // runtime's line already journals it (LLP 1175 D5). prelude.js takes it.
    global.__exact_uncaught = function (prefix, value) { writeLog(prefix + render(value), 'uncaught'); };
  }
})(globalThis);
