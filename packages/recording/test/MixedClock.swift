import AVFoundation
import ExactPackage

private extension MixedWriter {
    func enabled(_ value: Bool, at time: @autoclosure () -> CMTime) { mute.enabled(value, at: .init(time())) }
}

extension MixedWriter {
    @discardableResult func audio(_ sample: CMSampleBuffer, through time: @autoclosure () -> CMTime? = nil) -> CMSampleBuffer? {
        audio(sample, reading: .init(time() ?? (sample.presentationTimeStamp + sample.duration)))
    }
}

extension VideoTest {
    static func clockLag(_ directory: URL) async throws {
        let pause = 7200..<21600
        let base = 48000 * 3600
        let stamp = { (i: Int) in CMTime(value: Int64(base + i), timescale: 48000) }
        func run(_ name: String, lag: Int, reference: Bool) async throws -> ([Float], [Float]) {
            let dir = directory.appendingPathComponent(name)
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            let stage = dir.appendingPathComponent(".take.mp4")
            var admitted = false
            var stalled = false
            let writer = MixedWriter(stage, ready: { $0.isReadyForMoreMediaData && ($0.mediaType != .audio || !stalled) }, accepted: { _ in admitted = true }, failed: { preconditionFailure($0) })
            writer.configure(nil)
            if !reference { writer.enabled(false, at: .negativeInfinity) }
            var next = 0, during: [Float] = []
            var muted = true, enable = Int.max
            for tick in stride(from: 0, through: 240000, by: 480) {
                writer.queue.sync { stalled = (4800..<(4800 + lag)).contains(tick) || (24000..<(24000 + lag)).contains(tick) }
                for (frame, enabled) in [(2317, true), (4801, false), (9599 + pause.count, true)]
                    where tick <= frame && frame < tick + 480 {
                    muted = !enabled
                    enable = enabled ? frame : Int.max
                    if !reference { writer.enabled(enabled, at: stamp(frame)) }
                }
                if tick == pause.lowerBound { writer.pause(at: stamp(tick)) }
                if tick == pause.upperBound { writer.resume(at: stamp(tick)) }
                // The capture callback keeps masking while the audio encoder is stalled.
                while next + 480 <= tick {
                    if next % 2400 == 0 {
                        let frame = try MixedTake.agentPicture(base + next)
                        writer.queue.sync { writer.video(frame) }
                    }
                    writer.audio(try packet(base + next, 480) { i in reference && (muted || i - base <= enable + 480) }, through: stamp(tick + 480))
                    writer.queue.sync {}
                    next += 480
                }
                if tick == 192000 + lag {
                    let names = try FileManager.default.contentsOfDirectory(atPath: dir.path)
                    precondition(names == [stage.lastPathComponent])
                    let deadline = ContinuousClock.now + .seconds(4)
                    repeat {
                        during = (try? decoded(stage)) ?? []
                        if during.count > 12000 { break }
                        try await Task.sleep(for: .milliseconds(10))
                    } while ContinuousClock.now < deadline
                    precondition(during.count > 12000, "no readable fragment during capture")
                }
            }
            precondition(admitted)
            let names = try FileManager.default.contentsOfDirectory(atPath: dir.path)
            precondition(names == [stage.lastPathComponent])
            let error: String? = await withCheckedContinuation { done in
                writer.finish(publish: true) { message, _ in done.resume(returning: message) }
            }
            precondition(error == nil, "\(String(describing: error))")
            try await inspectMixed(stage)
            return (try decoded(stage), during)
        }
        let reference = try await run("zero-source", lag: 0, reference: true)
        for lag in [960, 9600] {
            let actual = try await run("lag-\(lag)", lag: lag, reference: false)
            for (samples, control) in [(actual.0, reference.0), (actual.1, reference.1)] {
                precondition(samples.count > 12000 && control.count > 12000)
                for window in [0..<2317, 4801..<9599] {
                    let peak = samples[window].map(abs).max()!
                    let delta = window.map { abs(samples[$0] - control[$0]) }.max()!
                    print("lag \(lag), window \(window), peak \(peak), difference \(delta)")
                    precondition(peak < 0.001 && delta < 1 / 32768.0)
                }
                precondition(samples[10800..<12000].map(abs).max()! > 0.4)
            }
            precondition(abs(actual.0.count - (240000 - pause.count)) < 1024)
        }
        print("PASS Apple mixed capture-clock mute in the staging and finished MP4")
    }

    static func queuedEdge(_ directory: URL) async throws {
        func encode(_ reference: Bool) async throws -> [Float] {
            var blocked = false
            let url = directory.appendingPathComponent(reference ? "reference.mp4" : "queued.mp4")
            let w = MixedWriter(url, ready: { $0.isReadyForMoreMediaData && ($0.mediaType != .audio || !blocked) },
                                accepted: { _ in }, failed: { preconditionFailure($0) })
            let window = 4801..<14400
            w.configure(nil)
            let pic = try MixedTake.agentPicture(0)
            w.queue.sync { w.video(pic) }
            w.audio(try packet(0, 4800))
            w.queue.sync { blocked = true }
            let captured = try packet(4800, 9600) { reference && window.contains($0) }
            let through = CMTime(value: reference ? 14400 : 4801, timescale: 48000)
            let shared = w.audio(captured, through: through)!
            let samples = try pcmSamples(shared)
            precondition(samples[1...].allSatisfy { $0 == 0 })
            w.queue.sync {}
            if !reference {
                w.enabled(false, at: CMTime(value: 4801, timescale: 48000))
                w.enabled(true, at: CMTime(value: 9599, timescale: 48000))
            }
            w.queue.sync { blocked = false }
            let error: String? = await withCheckedContinuation { done in
                w.finish(publish: true) { message, _ in done.resume(returning: message) }
            }
            precondition(error == nil)
            return try decoded(url)
        }
        let actual = try await encode(false), control = try await encode(true)
        precondition(actual == control && actual[4801..<9599].map(abs).max()! < 0.001)
        print("PASS Apple mixed reader and writer silence unproven samples before enqueue")
    }

    static func pcmSamples(_ sample: CMSampleBuffer) throws -> [Float] {
        let pcm = AVAudioPCMBuffer(pcmFormat: AVAudioFormat(cmAudioFormatDescription: sample.formatDescription!),
                                   frameCapacity: AVAudioFrameCount(sample.numSamples))!
        pcm.frameLength = pcm.frameCapacity
        precondition(CMSampleBufferCopyPCMDataIntoAudioBufferList(sample, at: 0, frameCount: Int32(sample.numSamples),
                      into: pcm.mutableAudioBufferList) == noErr)
        return Array(UnsafeBufferPointer(start: pcm.floatChannelData![0], count: sample.numSamples))
    }

    static func proofOrder(_ dir: URL) async throws {
        let stamp = { (i: Int) in CMTime(value: Int64(i), timescale: 48000) }
        let url = dir.appendingPathComponent("ordered.mp4")
        let writer = MixedWriter(url, accepted: { _ in }, failed: { preconditionFailure($0) })
        writer.configure(nil)
        let first = try pcmSamples(writer.audio(try packet(0, 4800), through: stamp(4800))!)
        precondition(first.contains { abs($0) > 0.4 })
        writer.enabled(false, at: .invalid)
        let second = try pcmSamples(writer.audio(try packet(4800, 4800), through: .invalid)!)
        precondition(second.allSatisfy { $0 == 0 })
        let picture = try MixedTake.agentPicture(0)
        writer.queue.sync { writer.video(picture) }
        await withCheckedContinuation { done in writer.finish(publish: true) { err, _ in precondition(err == nil); done.resume() } }
        let file = try decoded(url)
        precondition(file[1000..<4000].contains { abs($0) > 0.4 } && file[6000..<9000].allSatisfy { abs($0) < 0.001 })

        let concurrent = ExactAudioMute()
        concurrent.enabled(false, at: { preconditionFailure("disable read a clock") }())
        let heard = concurrent.silence(from: stamp(0), frames: 9600, rate: 48000, through: .init(stamp(9600)))
        precondition(heard == [0..<9600])
        print("PASS Apple mixed edge and capture proof share one order")
    }

    static func lostClock(_ dir: URL) async throws {
        let url = dir.appendingPathComponent("recovered.mp4")
        let writer = MixedWriter(url, accepted: { _ in }, failed: { preconditionFailure($0) })
        let stamp = { (frame: Int) in CMTime(value: Int64(frame), timescale: 48000) }
        func send(_ first: Int, _ clock: CMTime, sound: Range<Int>? = nil) throws {
            let input = try packet(first, 480)
            let shared = try pcmSamples(writer.audio(input, through: clock)!)
            let original = try pcmSamples(input)
            for i in shared.indices {
                precondition(shared[i] == (sound?.contains(first + i) == true ? original[i] : 0))
            }
            writer.queue.sync {}
        }
        writer.configure(nil)
        let frame = try MixedTake.agentPicture(0)
        writer.queue.sync { writer.video(frame) }
        writer.enabled(false, at: stamp(117))
        try send(0, stamp(480))
        try send(480, .invalid)
        writer.pause(at: stamp(960))
        writer.enabled(true, at: .invalid)
        try send(960, .invalid)
        // The first recovered clock reads mid-packet. That whole packet stays silent.
        try send(1440, stamp(1700))
        writer.resume(at: stamp(1920))
        let resumed = try MixedTake.agentPicture(1920)
        writer.queue.sync { writer.video(resumed) }
        try send(1920, stamp(2400))
        writer.enabled(false, at: .invalid)
        try send(2400, stamp(2880))
        writer.enabled(true, at: stamp(3101))
        try send(2880, stamp(3360))
        try send(3360, stamp(3840), sound: 3582..<3840)
        let error: String? = await withCheckedContinuation { done in
            writer.finish(publish: true) { message, _ in done.resume(returning: message) }
        }
        precondition(error == nil)
        let separate = MixedWriter(dir.appendingPathComponent("unopened.mp4"), accepted: { _ in }, failed: { preconditionFailure($0) })
        separate.enabled(false, at: stamp(960))
        _ = separate.audio(try packet(960, 480), through: .invalid)
        let recovered = try pcmSamples(separate.audio(try packet(0, 480), through: stamp(480))!)
        precondition(recovered.allSatisfy { $0 == 0 })
        let saved = try decoded(url)
        precondition(saved.count == 2880 && saved[360..<750].allSatisfy { abs($0) < 0.001 })
        print("PASS Apple mixed missing and recovered capture clock across a pause")
    }

    static func disjoint(_ dir: URL) async throws {
        let url = dir.appendingPathComponent("overlap.mp4")
        var admitted = false
        let w = MixedWriter(url, accepted: { _ in admitted = true }, failed: { preconditionFailure($0) })
        w.configure(nil)
        w.audio(try packet(0, 480)); w.audio(try packet(9600, 480))
        let gap = try MixedTake.agentPicture(2400), overlap = try MixedTake.agentPicture(9600)
        w.queue.sync { w.video(gap); precondition(!admitted); w.video(overlap) }
        await withCheckedContinuation { done in w.finish(publish: true) { error, _ in precondition(error == nil); done.resume() } }
        try await inspectMixed(url)
    }

    static func lostPicture(_ dir: URL) async throws {
        let url = dir.appendingPathComponent("tail.mp4")
        let w = MixedWriter(url, accepted: { _ in }, failed: { preconditionFailure($0) })
        w.configure(nil)
        let first = try MixedTake.agentPicture(0), second = try MixedTake.agentPicture(4800)
        w.queue.sync { w.video(first) }
        w.audio(try packet(0, 4800))
        w.queue.sync { w.video(second) }
        w.pause(at: CMTime(value: 9600, timescale: 48000))
        _ = w.audio(try packet(4800, 4800))
        await withCheckedContinuation { done in w.finish(publish: true) { e, _ in precondition(e == nil); done.resume() } }
        let asset = AVURLAsset(url: url), reader = try AVAssetReader(asset: asset)
        let track = try await asset.loadTracks(withMediaType: .video)[0]
        let video = AVAssetReaderTrackOutput(track: track, outputSettings: [kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA])
        reader.add(video); precondition(reader.startReading())
        var channels: [UInt8] = [], times: [Double] = []
        while let s = video.copyNextSampleBuffer(), let pixel = s.imageBuffer {
            CVPixelBufferLockBaseAddress(pixel, .readOnly)
            channels.append(CVPixelBufferGetBaseAddress(pixel)!.load(as: UInt8.self))
            CVPixelBufferUnlockBaseAddress(pixel, .readOnly)
            times.append(s.presentationTimeStamp.seconds)
        }
        let audio = try decoded(url)
        precondition(audio.count == 9600 && times == [0, 0.1] && channels[0] < channels[1])
        print("PASS Apple mixed pause kept both pictures and their 9600 audio frames")
    }

    static func discontinuities(_ dir: URL) async throws {
        func t(_ n: Int) -> CMTime { CMTime(value: Int64(n), timescale: 48000) }
        for caseName in ["slower", "faster", "missing", "frozen", "interruption", "replacement"] {
            let url = dir.appendingPathComponent(caseName + ".mp4")
            let w = MixedWriter(url, accepted: { _ in }, failed: { preconditionFailure($0) })
            w.configure(nil)
            let picture = try MixedTake.agentPicture(0)
            w.queue.sync { w.video(picture) }
            let host = CMClockGetHostTimeClock()
            let capture = try CMTimebase(sourceClock: host)
            let rate = caseName == "slower" ? 0.9999 : 1.0001
            try capture.setRateAndAnchorTime(rate: rate, anchorTime: t(0), referenceTime: t(0))
            // Only the session clock enters the masker, independent of its rate against the host.
            let stamp = { (frame: Int) in CMSyncConvertTime(t(frame), from: host, to: capture) }
            w.enabled(false, at: stamp(480))
            var shared: [Float] = []
            func send(_ first: Int, _ n: Int, _ now: CMTime, quiet: (Int) -> Bool = { _ in false }) throws {
                shared += try pcmSamples(w.audio(try packet(first, n, quiet: quiet), through: now)!)
            }
            switch caseName {
            case "slower", "faster":
                try send(0, 4800, stamp(5000)) { $0 != 480 }
            case "missing":
                w.enabled(true, at: .invalid)
                try send(0, 480, t(100))
                try send(480, 4320, t(4800))
            case "frozen":
                try send(0, 480, t(480))
                w.enabled(true, at: t(480))
                try send(480, 4320, t(480))
                try send(4800, 480, t(5280))
            default:
                try send(0, 480, t(480))
                w.enabled(true, at: t(600))
                if caseName == "interruption" { w.mute.discontinuity() }
                else { _ = w.mute.silence(from: t(480), frames: 1, rate: 48000, through: .init(clock: host)) }
                try send(960, 480, t(1440))
                try send(480, 480, t(1920))
            }
            await withCheckedContinuation { done in w.finish(publish: true) { error, _ in precondition(error == nil); done.resume() } }
            let decoded = try decoded(url)
            precondition(!decoded.isEmpty && shared.allSatisfy { $0 == 0 } && decoded.allSatisfy { abs($0) < 0.001 }, caseName)
            print("PASS Apple mixed", caseName, "shared and decoded peaks", shared.map(abs).max()!, decoded.map(abs).max()!)
        }
    }

}
