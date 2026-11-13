# A web calendar file inserts UID, DURATION, and URL with raw breaks

**Status:** Closed
**Resolution:** UID is written as escaped TEXT, URL and DURATION lose any CR or LF, and TEXT escaping turns a lone CR into `\n` as well, so no value breaks its content line.
**Systems:** packages/calendar
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`text()` escapes backslash, semicolon, comma, and newlines for `SUMMARY`, `DESCRIPTION`, and `LOCATION`. `UID`, `DURATION`, and `URL` are interpolated raw. `duration` is a free string. `url` and `uid` are optional strings.

A CR or LF in any of those three splits the content line. The downloaded `.ics` can carry extra properties, or a second `VEVENT`, into the system calendar. The web suite checks TEXT escaping only.

## Where

`packages/calendar/web/index.js`, `text` and `ics`.

## How to reproduce

Show an event whose `url`, `uid`, or `duration` contains a newline and then `SUMMARY:injected` or `END:VEVENT` followed by another event. Open the downloaded file. The extra property or event is present.

## Direction

Reject or strip CR and LF on every property. Keep TEXT escaping for summary, description, and location. Treat URL, UID, and DURATION as single-line values.
