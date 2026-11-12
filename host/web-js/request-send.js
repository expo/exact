import { counted, longPollEnded, longPollReplied, trackAnswer, Open } from "./held.js";
import { failureCode, failureReason } from "./shape.js";

export function sendRequest(t, land, out, data, inflight, say) {
  out.push(() => {
    t.waiting = true; counted(t, inflight);
    const started = performance.now();
    // A stream's message (`more`) keeps its ticket; anything else ends it.
    const done = (o, more) => {
      if (t.closed) return;
      if (t.waiting && !(more && t.upload)) { t.waiting = false; counted(t, inflight); } // an upload is in flight until its reply
      // A render's storage refusal starts no work and leaves the source pending for adoption (LLP 1048.000 D9).
      if (o.refused) { t.refused = o.refused; return; }
      t.elapsed = Math.max(0, Math.round(performance.now() - started));
      if (more) { t.messages++; t.coalesced += (o.streamed ?? o).coalesced ?? 0; o.more = true; } else { Open.delete(t); longPollEnded(t); }
      say(`${more ? "message" : "reply"} ${t.id}; wall ${t.elapsed} ms`); land(o);
    };
    const failed = e => done({ failed: 1, message: String(e?.message ?? e) });
    // An answer that keeps coming (LLP 1016.000): a Rust source's streamed request, or a TypeScript source's `exactStream`
    // (ts-data.js). In flight until its first message (D5), then open until its end or until its ticket is let go (`Open`).
    if (t.stream || t.req?.stream) {
      t.ctl = new AbortController(); t.ctl.forced = t.forced; t.ctl.recount = () => counted(t, inflight); t.ctl.active = () => t.held() && !t.r?.gone; t.ctl.resource = !!t.r; t.messages = t.coalesced = 0; Open.add(t); // `resource`: a socket it owns takes its mapper's replies (ts-stream.js)
      t.ctl.replied = () => { if (t.waiting) { t.waiting = false; counted(t, inflight); } longPollReplied(t); }; // a long poll's reply of any kind (LLP 1016.000 D8 on D9): out of the clocks' count, held, a reply for `clock held`
      (t.stream ? t.stream(o => done(o, true), t.ctl) : data.fetch(t.req, m => (t.req.longPoll && m.event === 'exact:reply') || done({ streamed: m }, true), t.ctl)).then(done, failed);
    } else if (t.req) data.fetch(t.req).then(done, failed);
    else { if (globalThis.__exactRender) trackAnswer(t); t.promise.then(v => done({ v }), e => done({ error: String(e?.message ?? e), code: failureCode(e), reason: failureReason(e) })); }
  });
}
