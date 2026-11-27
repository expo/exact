#if os(iOS)
import AVFoundation
import Foundation

final class VideoBackgroundPlayback {
    static let configured = {
        let info = Bundle.main.infoDictionary
        let session = info?["ExactAudioSession"] as? String ?? ""
        return ["playback", "playback-mix"].contains(session) && (info?["UIBackgroundModes"] as? [String])?.contains("audio") == true
    }()

    private weak var asset: AVAsset?
    private var audibleGroup: AVMediaSelectionGroup?
    private var read: Task<Void, Never>?

    deinit { read?.cancel() }

    func prepare(_ next: AVAsset) {
        guard asset !== next else { return }
        asset = next
        audibleGroup = nil
        read?.cancel()
        read = Task { @MainActor [weak self] in
            let group = try? await next.loadMediaSelectionGroup(for: .audible)
            guard !Task.isCancelled, let self, self.asset === next else { return }
            self.audibleGroup = group
        }
    }

    func hasAudio(_ item: AVPlayerItem?) -> Bool {
        guard let item else { return false }
        if item.asset === asset, let audibleGroup {
            return item.currentMediaSelection.selectedMediaOption(in: audibleGroup) != nil
        }
        return Self.hasTrack(.audio, in: item)
    }

    // On iOS 27 AVFoundation pauses a playing video itself. A pending play, an
    // audio-only item and an AVQueuePlayer on iOS 26.5 need an explicit pause.
    static func willPauseAtHome(_ player: AVPlayer) -> Bool {
        if #available(iOS 27, *) {
            return player.audiovisualBackgroundPlaybackPolicy == .pauses && player.timeControlStatus == .playing
                && hasTrack(.video, in: player.currentItem)
        }
        return false
    }

    private static func hasTrack(_ type: AVMediaType, in item: AVPlayerItem?) -> Bool {
        item?.tracks.contains { $0.isEnabled && $0.assetTrack?.mediaType == type } == true
    }
}
#endif
