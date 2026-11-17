// @ref LLP 1133.024 D4, amended 2026-11-15 — what Safari runs in a page it
// shares to an app that maps `url`, before the share extension starts. It
// answers the page's title; as `url` its canonical link, or its location
// when it names no canonical web link; and as `location` its location, when
// that is not the `url`. The extension writes `url` and `title` as the
// share's when the share is of that location (ShareInbox.swift, `merge`). A
// canonical link that would take an https page to http is not taken. No URL
// keeps its userinfo, a name and password that would otherwise reach the
// app. A member with no value is left out. The extension reads only an
// answer of at most 1 MB, so the title is cut at `limit` UTF-16 units, a
// canonical link longer than that is not taken, and a location longer than
// `most` is not sent as `location`: the extension then compares with the
// `url`. The script is Exact's and the same in every app. Safari calls `run`
// of the global it must find under this name ("Accessing a Webpage",
// https://developer.apple.com/library/archive/documentation/General/Conceptual/ExtensibilityPG/ExtensionScenarios.html).
var ExtensionPreprocessingJS = {
  run: function (parameters) {
    var limit = 4096;
    // The extension's 1 MB (`pageCap`), less room for a title of `limit` units, a canonical link and the list's keys.
    var most = (1 << 20) - 4 * limit;
    // `href` as a web URL without its userinfo, or null.
    function web(href) {
      try {
        var url = new URL(href);
        if (!/^https?:$/.test(url.protocol)) return null;
        url.username = url.password = '';
        return url.href;
      } catch (error) {
        return null;
      }
    }
    var here = web(document.location.href);
    var link = document.querySelector('link[rel~="canonical" i][href]');
    var canonical = link && web(link.href);
    if (canonical && (canonical.length > limit || (/^https:/.test(here) && !/^https:/.test(canonical)))) canonical = null;
    var title = String(document.title).slice(0, limit);
    // A cut that splits a surrogate pair drops its first half too.
    if (/[\uD800-\uDBFF]$/.test(title)) title = title.slice(0, -1);
    var answer = { title: title };
    var url = canonical || here;
    if (url) answer.url = url;
    if (here && here !== url && here.length <= most) answer.location = here;
    parameters.completionFunction(answer);
  },
};
