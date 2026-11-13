// Compiled with the Apple module's sources by sessions.test.mjs: a session's
// bookkeeping across responses, run on the agent model (§9), so no window,
// download or Foundation Models generation is involved.
import ExactPackage
import Foundation

@main
private struct SessionTests {
    static var failures: [String] = []
    static func check(_ what: String, _ ok: Bool, _ got: Any = "") { if !ok { failures.append("\(what): got \(got)") } }

    final class Clock { var now = 0.0 }

    /// A module on the agent model whose replies are `replies` (§9), its session clock `clock`.
    static func module(_ clock: Clock, replies: [[String: Any]], rate: Double = 25) -> LanguageModelModule {
        let context = ExactPackageContext(agent: true, session: "sessions", changed: { _ in }, log: { _ in }, now: { clock.now })
        let m = LanguageModelModule(context: context)
        m.preferred(["responses": replies, "rate": rate])
        return m
    }

    static func ask(_ m: LanguageModelModule, _ name: String, _ args: [String: Any]) -> [String: Any]? {
        var answer: [String: Any]?
        m.later(name, args, preferred: nil) { answer = $0 }
        return answer
    }

    static func session(_ m: LanguageModelModule) -> ModelSession {
        let id = ask(m, "createLanguageModel", [:])?["id"] as? String ?? ""
        return m.sessions[id]!
    }

    /// An abort of the response generating leaves no platform session for the next prompt, which is
    /// already queued, to reuse: Foundation Models' would lack the aborted turn.
    static func abortedPlatform() {
        for (after, partial) in [(0.0, ""), (60.0, "one two ")] {
            let clock = Clock()
            let m = module(clock, replies: [["text": "one two three", "hold": true], ["text": "next", "hold": true]])
            let s = session(m)
            let first = ask(m, "promptStreaming", ["session": s.id, "input": "first"])?["id"] as? String ?? ""
            _ = ask(m, "promptStreaming", ["session": s.id, "input": "second"])
            s.platform = NSObject()
            s.stale = false
            clock.now = after
            _ = ask(m, "languageModelResponse", ["response": first])
            _ = ask(m, "abortLanguageModelResponse", ["response": first])
            check("abort after \(after) ms: the platform session is not kept", s.platform == nil, s.platform as Any)
            check("abort after \(after) ms: the aborted turn keeps its partial reply", s.turns.first?.reply == partial, s.turns.first?.reply as Any)
            check("abort after \(after) ms: the queued prompt began", s.responding?.turn.input == "second", s.responding?.turn.input as Any)
        }
    }

    /// The end of a session (destroyed, its handle dropped, or the module's end) answers each call
    /// waiting behind its response once, with AbortError, and begins none: no append lands, and no
    /// clone survives it.
    static func endedQueue() {
        let ends: [(String, (LanguageModelModule, ModelSession) -> Void)] = [
            ("destroy", { m, s in _ = ask(m, "destroyLanguageModel", ["session": s.id]) }),
            ("dispose", { m, s in m.dispose([s.id]) }),
            ("module destroy", { m, _ in m.destroy {} }),
        ]
        for (how, end) in ends {
            let m = module(Clock(), replies: [["text": "held", "hold": true]])
            let s = session(m)
            _ = ask(m, "promptStreaming", ["session": s.id, "input": "first"])
            var answers: [String: [[String: Any]]] = [:]
            for (name, args) in [("prompt", ["input": "queued"]), ("appendLanguageModel", ["input": "appended"]), ("cloneLanguageModel", [:])] {
                m.later(name, args.merging(["session": s.id]) { $1 }, preferred: nil) { answers[name, default: []].append($0) }
            }
            check("\(how): nothing queued has answered", answers.isEmpty, answers)
            let turns = s.turns.count
            end(m, s)
            for name in ["prompt", "appendLanguageModel", "cloneLanguageModel"] {
                let got = answers[name] ?? []
                check("\(how): \(name) answered once, aborted", got.count == 1 && got[0]["error"] as? String == "AbortError", got)
            }
            check("\(how): no append landed", s.turns.count == turns, s.turns.map(\.input))
            check("\(how): no session survives", m.sessions.isEmpty, Array(m.sessions.keys))
        }
    }

    /// A response is kept by its call's request only while it is live, for that call's cancel: a
    /// hundred completed streams leave none, and nor does the end of their session.
    static func requestsReleased() {
        let m = module(Clock(), replies: [], rate: 0)
        let s = session(m)
        for request in 1...100 {
            m.context.request = request
            let id = ask(m, "promptStreaming", ["session": s.id, "input": "question \(request)"])?["id"] as? String ?? ""
            _ = ask(m, "languageModelResponse", ["response": id])
        }
        check("completed streams keep no request", m.byRequest.isEmpty, m.byRequest.count)
        check("eight ended responses are kept", m.responses.count == LanguageModelModule.kept, m.responses.count)

        // A live one is kept until its call is let go, which still aborts it.
        let held = module(Clock(), replies: [["text": "held", "hold": true]])
        let t = session(held)
        held.context.request = 7
        let id = ask(held, "promptStreaming", ["session": t.id, "input": "held"])?["id"] as? String ?? ""
        check("a live stream is kept by its request", held.byRequest[7] != nil, held.byRequest.count)
        held.cancel(7)
        check("letting its call go aborts it", held.responses[id]?.state == "aborted", held.responses[id]?.state as Any)
        check("and releases its request", held.byRequest.isEmpty, held.byRequest.count)

        held.context.request = 8
        _ = ask(held, "promptStreaming", ["session": t.id, "input": "held again"])
        _ = ask(held, "destroyLanguageModel", ["session": t.id])
        check("a destroyed session's streams keep no request", held.byRequest.isEmpty && held.responses.isEmpty,
              "\(held.byRequest.count) \(held.responses.count)")
    }

    static func main() {
        abortedPlatform()
        endedQueue()
        requestsReleased()
        for failure in failures { print("FAIL: \(failure)") }
        print("language-model sessions: \(failures.count) failures")
        exit(failures.isEmpty ? 0 : 1)
    }
}
