// The store (LLP 1018), for prelude.js: reads counted, writes grant-checked,
// in Rust; its secrets' leases (`exclusive`) and kept keys (crypto.js).
// prelude.js takes `__exact_store` and deletes it. `env` is the prelude's
// door and turn: the answer running, a call by its id, and the overlay's
// refusal.
(function (global) {
  "use strict";
  global.__exact_store = function (env) {
    var host = env.host,
      browserSecrets = env.browserSecrets,
      inOverlay = env.inOverlay;
    var presence = env.presence(
      host,
      function (name) {
        return store.get(name);
      },
      function (api) {
        if (env.overlaying()) throw inOverlay(api);
      }
    );
    var store = {
      get: function (name) {
        if (env.overlaying()) throw inOverlay("store.get()");
        var v = presence.door(2, String(name), "");
        return v === undefined ? null : v;
      },
      set: function (name, value) {
        if (env.overlaying()) throw inOverlay("store.set()");
        var e = secretWrite(name, String(value)) ? undefined : presence.door(3, String(name), String(value));
        if (e !== undefined) throw new Error(e);
      },
      forget: function (name) {
        if (env.overlaying()) throw inOverlay("store.forget()");
        var e = secretWrite(name, null) ? undefined : host(4, String(name), "");
        if (e !== undefined) throw new Error(e);
      },
      has: presence.has,
      locked: presence.locked,
      unlock: presence.unlock,
      protect: presence.protect,
      unprotect: presence.unprotect,
      keepKey: function (name, pair) {
        return env.keys.keepKey(name, pair);
      },
      key: function (name) {
        return env.keys.keptKey(name);
      },
    };

    function secretWrite(name, value) {
      var currentCall = env.current();
      var task = currentCall && currentCall.secrets && currentCall.secrets[0];
      if (browserSecrets || !task || !task.held || task.name !== String(name)) return false;
      host(26, String(currentCall.id), JSON.stringify({ name: String(name), value: value }));
      return true;
    }
    store.exclusive = function (name, work) {
      if (env.overlaying()) return Promise.reject(inOverlay("store.exclusive()"));
      var call = env.current();
      if (!call || call.replied || call.letGo)
        return Promise.reject(new Error("store.exclusive() called outside an answer"));
      if (!browserSecrets && call.secrets && call.secrets.length)
        return Promise.reject(new Error("nested secret leases are unsupported"));
      return new Promise(function (resolve, reject) {
        if (!browserSecrets) host(24, String(call.id), String(name));
        (call.secrets || (call.secrets = [])).push({
          name: String(name),
          work: work,
          resolve: resolve,
          reject: reject,
        });
      });
    };
    global.__exact_secret_ready = function (id, error) {
      var call = env.call(id),
        task = call && call.secrets && call.secrets[0];
      if (!task || call.letGo) throw new Error("the secret's answer was let go");
      env.setCurrent(call);
      task.held = true;
      var finish = function (ok, value) {
        if (!browserSecrets) {
          try {
            host(25, String(call.id), ok ? "commit" : "");
          } catch (e) {
            ok = false;
            value = e;
          }
        }
        call.secrets.shift();
        (ok ? task.resolve : task.reject)(value);
      };
      if (error) {
        finish(false, new Error(error));
        return;
      }
      try {
        Promise.resolve(task.work()).then(
          function (v) {
            finish(true, v);
          },
          function (e) {
            finish(false, e);
          }
        );
      } catch (e) {
        finish(false, e);
      }
    };
    return store;
  };
})(globalThis);
