// URL and URLSearchParams.
//
// The CLASS SHAPE is here; every question about a URL is answered in Rust
// (stdlib/url.rs over the `url` crate — LLP 0059.000 §3.4: a partial URL
// parser is a security bug, not a missing feature). A URL object holds the
// eleven components the parser returned, and a setter asks Rust for the URL
// as it is after the assignment, so the setters' semantics are the spec's.
//
// A URLSearchParams holds its serialized query string. Every method is one
// crossing that reads or rewrites that string in Rust, so the list semantics
// — order, duplicates, form-urlencoded escaping — have one implementation,
// and `url.searchParams` is a live view because it reads and writes
// `url.search` directly rather than keeping a copy. Iteration alone keeps the
// pairs Rust parsed, until the query changes.
(function (global) {
  "use strict";

  var brand = global.__ibex2_brand || function (value) { return value; };

  // Captured, then removed from the global object, so that after boot the
  // global carries the classes and nothing underneath them.
  var parse = global.__ibex2_url_parse;
  var setField = global.__ibex2_url_set;
  var sp = {
    normalize: global.__ibex2_search_params_normalize,
    get: global.__ibex2_search_params_get,
    getAll: global.__ibex2_search_params_get_all,
    has: global.__ibex2_search_params_has,
    set: global.__ibex2_search_params_set,
    append: global.__ibex2_search_params_append,
    remove: global.__ibex2_search_params_delete,
    sort: global.__ibex2_search_params_sort,
    entries: global.__ibex2_search_params_entries,
  };
  [
    "__ibex2_url_parse", "__ibex2_url_set", "__ibex2_search_params_normalize",
    "__ibex2_search_params_get", "__ibex2_search_params_get_all", "__ibex2_search_params_has",
    "__ibex2_search_params_set", "__ibex2_search_params_append", "__ibex2_search_params_delete",
    "__ibex2_search_params_sort", "__ibex2_search_params_entries",
  ].forEach(function (name) { delete global[name]; });

  var FIELDS = [
    "href", "origin", "protocol", "username", "password", "host", "hostname",
    "port", "pathname", "search", "hash",
  ];
  var SEARCH = FIELDS.indexOf("search");

  // Rust reports a parse failure as a message beginning "TypeError: "; the
  // spec wants an actual TypeError.
  function asTypeError(e) {
    var message = String(e && e.message !== undefined ? e.message : e);
    return new TypeError(message.replace(/^TypeError: /, ""));
  }

  var urls = new WeakMap(); // URL -> components array
  var views = new WeakMap(); // URL -> its searchParams, created once
  var params = new WeakMap(); // URLSearchParams -> { url: URL | null, query: string }

  function components(input, base) {
    try {
      return (base === undefined ? parse(String(input)) : parse(String(input), String(base))).split("\n");
    } catch (e) {
      throw asTypeError(e);
    }
  }

  function state(url) {
    var c = urls.get(url);
    if (!c) throw new TypeError("not a URL");
    return c;
  }

  function URL(input, base) {
    if (!(this instanceof URL)) throw new TypeError("URL must be constructed with new");
    if (arguments.length === 0) throw new TypeError("URL constructor requires a URL");
    urls.set(this, components(input, base));
    brand(this, "URL");
  }

  FIELDS.forEach(function (field, index) {
    var descriptor = {
      enumerable: true,
      configurable: true,
      get: function () {
        return state(this)[index];
      },
    };
    if (field !== "origin") {
      descriptor.set = function (value) {
        var c = state(this);
        var next;
        try {
          next = setField(c[0], field, String(value));
        } catch (e) {
          // Only `href` throws; every other setter fails silently, and Rust
          // has already returned the URL unchanged for those.
          if (field === "href") throw asTypeError(e);
          return;
        }
        urls.set(this, next.split("\n"));
      };
    }
    Object.defineProperty(URL.prototype, field, descriptor);
  });

  Object.defineProperty(URL.prototype, "searchParams", {
    enumerable: true,
    configurable: true,
    get: function () {
      state(this);
      var view = views.get(this);
      if (!view) {
        view = new URLSearchParams();
        params.get(view).url = this;
        views.set(this, view);
      }
      return view;
    },
  });

  URL.prototype.toString = function () {
    return state(this)[0];
  };
  URL.prototype.toJSON = function () {
    return state(this)[0];
  };
  Object.defineProperty(URL.prototype, Symbol.toStringTag, { value: "URL", configurable: true });

  URL.canParse = function (input, base) {
    try {
      components(input, base);
      return true;
    } catch (e) {
      return false;
    }
  };
  URL.parse = function (input, base) {
    try {
      return new URL(input, base);
    } catch (e) {
      return null;
    }
  };

  // --- URLSearchParams ------------------------------------------------------

  function query(p) {
    if (p.url) return state(p.url)[SEARCH].replace(/^\?/, "");
    return p.query;
  }

  function write(p, next) {
    if (p.url) {
      // The spec's "update steps": the owner's search becomes the serialization,
      // and an empty one removes the `?` entirely.
      urls.set(p.url, setField(state(p.url)[0], "search", next).split("\n"));
    } else {
      p.query = next;
      p.parsed = null;
    }
  }

  // The pairs of the query as it is now, parsed once per change. A view's
  // URL replaces its components array on every change, however it was made;
  // a list of its own drops its pairs when it writes.
  function list(p) {
    var from = p.url ? state(p.url) : p;
    if (p.parsed !== from) {
      p.pairs = JSON.parse(sp.entries(query(p)));
      p.parsed = from;
    }
    return p.pairs;
  }

  function own(instance) {
    var p = params.get(instance);
    if (!p) throw new TypeError("not a URLSearchParams");
    return p;
  }

  function URLSearchParams(init) {
    if (!(this instanceof URLSearchParams)) {
      throw new TypeError("URLSearchParams must be constructed with new");
    }
    var p = { url: null, query: "", pairs: null, parsed: null };
    params.set(this, p);
    brand(this, "URLSearchParams");
    if (init === undefined || init === null) return;
    if (typeof init === "string") {
      p.query = sp.normalize(init.charAt(0) === "?" ? init.slice(1) : init);
      return;
    }
    if (init instanceof URLSearchParams) {
      p.query = query(own(init));
      return;
    }
    if (typeof init !== "object") {
      p.query = sp.normalize(String(init));
      return;
    }
    if (typeof init[Symbol.iterator] === "function") {
      var pairs = Array.from(init);
      for (var i = 0; i < pairs.length; i++) {
        var pair = Array.from(pairs[i]);
        if (pair.length !== 2) {
          throw new TypeError("URLSearchParams init sequence entries must be name/value pairs");
        }
        p.query = sp.append(p.query, String(pair[0]), String(pair[1]));
      }
      return;
    }
    var names = Object.keys(init);
    for (var j = 0; j < names.length; j++) {
      p.query = sp.append(p.query, names[j], String(init[names[j]]));
    }
  }

  var proto = URLSearchParams.prototype;

  proto.get = function (name) {
    return sp.get(query(own(this)), String(name));
  };
  proto.getAll = function (name) {
    return JSON.parse(sp.getAll(query(own(this)), String(name)));
  };
  proto.has = function (name, value) {
    var q = query(own(this));
    return arguments.length > 1 && value !== undefined
      ? sp.has(q, String(name), String(value))
      : sp.has(q, String(name));
  };
  proto.set = function (name, value) {
    var p = own(this);
    write(p, sp.set(query(p), String(name), String(value)));
  };
  proto.append = function (name, value) {
    var p = own(this);
    write(p, sp.append(query(p), String(name), String(value)));
  };
  proto.delete = function (name, value) {
    var p = own(this);
    var q = query(p);
    write(
      p,
      arguments.length > 1 && value !== undefined
        ? sp.remove(q, String(name), String(value))
        : sp.remove(q, String(name))
    );
  };
  proto.sort = function () {
    var p = own(this);
    write(p, sp.sort(query(p)));
  };
  proto.toString = function () {
    return query(own(this));
  };
  Object.defineProperty(proto, "size", {
    configurable: true,
    get: function () {
      return list(own(this)).length;
    },
  });

  // Iteration is live, as WebIDL's pair iterators are: each step reads the
  // pair at its index in the list as the list is then
  // (https://webidl.spec.whatwg.org/#es-iterator-prototype-object).
  var IteratorPrototype = Object.getPrototypeOf(
    Object.getPrototypeOf([][Symbol.iterator]())
  );
  var iterators = new WeakMap(); // iterator -> { p, index, kind }
  var IteratorProto = Object.create(IteratorPrototype);
  IteratorProto.next = function () {
    var it = iterators.get(this);
    if (!it) throw new TypeError("not a URLSearchParams iterator");
    var pairs = list(it.p);
    if (it.index >= pairs.length) return { done: true, value: undefined };
    var pair = pairs[it.index++];
    return {
      done: false,
      value: it.kind === 0 ? [pair[0], pair[1]] : it.kind === 1 ? pair[0] : pair[1],
    };
  };
  Object.defineProperty(IteratorProto, Symbol.toStringTag, {
    value: "URLSearchParams Iterator",
    configurable: true,
  });
  function iterator(instance, kind) {
    var it = Object.create(IteratorProto);
    iterators.set(it, { p: own(instance), index: 0, kind: kind });
    return brand(it, "URLSearchParamsIterator");
  }
  proto.entries = function () {
    return iterator(this, 0);
  };
  proto.keys = function () {
    return iterator(this, 1);
  };
  proto.values = function () {
    return iterator(this, 2);
  };
  proto[Symbol.iterator] = proto.entries;
  proto.forEach = function (callback, thisArg) {
    if (typeof callback !== "function") throw new TypeError("forEach requires a function");
    var p = own(this);
    for (var i = 0; i < list(p).length; i++) {
      var pair = list(p)[i];
      callback.call(thisArg, pair[1], pair[0], this);
    }
  };
  Object.defineProperty(proto, Symbol.toStringTag, { value: "URLSearchParams", configurable: true });

  global.URL = URL;
  global.URLSearchParams = URLSearchParams;
})(globalThis);
