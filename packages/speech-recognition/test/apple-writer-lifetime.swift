import Darwin
import ExactPackage
import Foundation

private final class NativeOwner {
    let transcript: URL
    init(_ transcript: URL) { self.transcript = transcript }
}

private final class Probe {
    var directoryDescriptor: Int32 = -1
}

@main
private struct WriterLifetime {
    static func main() {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("exact-speech-writer-\(UUID().uuidString)")
        let transcript = root.appendingPathComponent("take.transcript", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try? FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let entered = DispatchSemaphore(value: 0), release = DispatchSemaphore(value: 0)
        let probe = Probe()
        var owner: NativeOwner? = NativeOwner(transcript)
        let context = ExactPackageContext(
            agent: true, session: "writer-lifetime", changed: { _ in }, log: { _ in }, now: { 0 },
            directory: { [weak owner] _ in owner?.transcript },
            replaceDirectory: { [weak owner] _ in
                entered.signal()
                release.wait()
                guard let transcript = owner?.transcript else { return nil }
                try? FileManager.default.removeItem(at: transcript)
                do { try FileManager.default.createDirectory(at: transcript, withIntermediateDirectories: false) }
                catch { return nil }
                let fd = open(transcript.path, O_RDONLY | O_DIRECTORY | O_CLOEXEC)
                guard fd >= 0 else { return nil }
                probe.directoryDescriptor = fd
                return FileHandle(fileDescriptor: fd, closeOnDealloc: true)
            })
        let module = SpeechRecognitionModule(context: context)
        module.preferred(["heard": "final words"])
        module.later("startSpeechRecognition", [
            "path": "app:/data/take.wav", "transcript": "app:/data/take.transcript",
        ], preferred: nil) { _ in }
        guard entered.wait(timeout: .now() + 2) == .success else { return fail("writer did not start") }

        var destroyed = false
        module.destroy {
            owner = nil
            destroyed = true
        }
        var crossedMainQueue = false
        DispatchQueue.main.async { crossedMainQueue = true }
        let barrierDeadline = Date(timeIntervalSinceNow: 2)
        while !crossedMainQueue, Date() < barrierDeadline {
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.01))
        }
        guard crossedMainQueue else { return fail("main queue barrier did not run") }
        release.signal()
        let deadline = Date(timeIntervalSinceNow: 2)
        while !destroyed, Date() < deadline { RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.01)) }
        guard destroyed else { return fail("module destruction did not finish") }

        let file = transcript.appendingPathComponent("00000.jsonl")
        guard let data = try? Data(contentsOf: file), String(decoding: data, as: UTF8.self).contains("final words") else {
            return fail("final transcript was not written")
        }
        errno = 0
        guard probe.directoryDescriptor >= 0,
              fcntl(probe.directoryDescriptor, F_GETFD) == -1, errno == EBADF else {
            return fail("transcript directory descriptor stayed open")
        }
        print("final transcript written; directory closed")
    }

    private static func fail(_ message: String) {
        FileHandle.standardError.write(Data("\(message)\n".utf8))
        exit(1)
    }
}
