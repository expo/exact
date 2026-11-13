# The Jev dev proxy lets any website spend the gateway key

**Status:** Open
**Systems:** game/tennis
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

`jev-proxy.mjs` listens on 127.0.0.1:47913 and forwards every POST `/v1/evaluate` with `Authorization: Bearer` taken from `AI_GATEWAY_API_KEY`. The CORS headers allow every origin and send `access-control-allow-private-network: true`, which is the header Chrome's private-network preflight requires. While the proxy is running, a public page can post an evaluation body and read the gateway's response. The key is not in the game binary. This process attaches it.

## Where

`game/games/tennis/jev-proxy.mjs`.

## How to reproduce

Start the proxy with a real key. From a public https page, preflight and POST `http://127.0.0.1:47913/v1/evaluate`. The gateway is called with the key.

## Direction

Reject every Origin that is not the local dev page before adding the key. Do not send `access-control-allow-private-network` for any other caller.
