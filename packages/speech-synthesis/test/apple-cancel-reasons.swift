import ExactPackage
import Foundation

@main
private struct CancelReasonsTest {
    static func pump(until condition: () -> Bool, timeout: TimeInterval = 5.0) -> Bool {
        let deadline = Date(timeIntervalSinceNow: timeout)
        while !condition(), Date() < deadline {
            RunLoop.main.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01))
        }
        return condition()
    }

    static func main() {
        var claimEvents: ((String) -> Void)?
        var claimCount = 0
        var releaseCount = 0
        let audioSession = ExactAudioSession { _, _, _, events in
            claimCount += 1
            claimEvents = events
            return {
                releaseCount += 1
                claimEvents = nil
            }
        }
        let context = ExactPackageContext(
            agent: false, session: "speech-cancel-test",
            changed: { _ in }, log: { _ in }, now: { 0 },
            audioSession: audioSession
        )
        let module = SpeechSynthesisModule(context: context)
        defer { module.destroy {} }

        func ask(_ name: String, _ args: [String: Any] = [:]) -> [String: Any] {
            var answer: [String: Any]?
            module.later(name, args, preferred: nil) { answer = $0 }
            guard let answer else { fail("\(name) did not answer synchronously") }
            return answer
        }

        // 1. Speak silently so an audio session claim is acquired.
        _ = ask("speak", ["text": "hello world", "volume": 0.0])
        guard pump(until: { claimEvents != nil }) else {
            fail("audio session claim was not acquired")
        }

        // 2. Simulate an audio interruption, pausing the queue with reason "interrupted".
        claimEvents?("interrupted")
        let paused = ask("speechSynthesisState")
        guard paused["paused"] as? Bool == true,
              (paused["pauseReasons"] as? [String]) == ["interrupted"] else {
            fail("expected interrupted pause state, got \(paused)")
        }

        // 3. cancelSpeech must clear "interrupted" so a subsequent speak starts immediately.
        let cancelled = ask("cancelSpeech")
        guard cancelled["paused"] as? Bool == false,
              (cancelled["pauseReasons"] as? [String])?.isEmpty == true else {
            fail("cancelSpeech left pause reasons stuck: \(cancelled)")
        }
        let before = claimCount
        _ = ask("speak", ["text": "fresh utterance", "volume": 0.0])
        guard pump(until: { ask("speechSynthesisState")["speaking"] as? Bool == true }) else {
            fail("subsequent speak stalled after cancelSpeech: \(ask("speechSynthesisState"))")
        }
        guard claimCount > before else {
            fail("subsequent speak did not re-acquire audio claim")
        }
        let after = ask("speechSynthesisState")
        guard after["paused"] as? Bool == false,
              (after["pauseReasons"] as? [String])?.isEmpty == true,
              after["speaking"] as? Bool == true else {
            fail("expected speaking state after restart, got \(after)")
        }
        _ = ask("cancelSpeech")
        guard releaseCount >= 2 else { fail("expected audio session claims released") }
        print("speech-synthesis cancel reasons ok")
    }

    private static func fail(_ message: String) -> Never {
        FileHandle.standardError.write(Data("\(message)\n".utf8))
        exit(1)
    }
}
