// @ref LLP 1069.002 D8, LLP 1047.001.000 E — iOS's photo picker
// (`PHPickerViewController`; no permission, no usage string) for a file
// input whose `accept` is only images and videos. PhotosUI is in its own
// module because linking it costs every launch, before `main`, the loading
// of ~300 more images (~1 s in the simulator). An app's composition links
// this module only when its plan can show the media picker (`media_picker`)
// and installs it before any session is made; the core's `Picker` asks it
// for the sheet and keeps everything else. On macOS and tvOS installing it
// does nothing: the Mac uses `NSOpenPanel`, and tvOS has no picker.
import ExactKit

public enum ExactMediaPicker {
    public static func install() {
        #if os(iOS)
        MediaPickerLink.installed = MediaPicker()
        #endif
    }
}

#if os(iOS)
import PhotosUI
import UIKit
import UniformTypeIdentifiers

final class MediaPicker: MediaPickerCapability {
    func present(over controller: UIViewController, accept: [String], multiple: Bool, done: @escaping ([(url: URL, name: String)], String?) -> Void) {
        var config = PHPickerConfiguration()
        config.selectionLimit = multiple ? 0 : 1
        let images = accept.contains { $0.hasPrefix("image/") }, videos = accept.contains { $0.hasPrefix("video/") }
        config.filter = images && videos ? .any(of: [.images, .videos]) : images ? .images : .videos
        // D6: HEVC/ProRes stays as is under `video/*`; otherwise H.264.
        config.preferredAssetRepresentationMode = accept.contains("video/*") || !videos ? .current : .compatible
        let picker = PHPickerViewController(configuration: config)
        let delegate = Delegate(done)
        picker.delegate = delegate
        // The picker holds its delegate weakly; the delegate lives as long as it does.
        objc_setAssociatedObject(picker, &Delegate.key, delegate, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        controller.present(picker, animated: true)
    }

    private final class Delegate: NSObject, PHPickerViewControllerDelegate {
        nonisolated(unsafe) static var key = 0
        private var done: (([(url: URL, name: String)], String?) -> Void)?
        init(_ done: @escaping ([(url: URL, name: String)], String?) -> Void) { self.done = done }

        func picker(_ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]) {
            picker.dismiss(animated: true)
            guard let done else { return }
            self.done = nil
            MediaPicker.stage(results.map(\.itemProvider), done: done)
        }
    }

    /// Each chosen item's file, staged in the temporary directory: the
    /// provider's URL is deleted when its callback returns, so a copy is
    /// staged there, for the core to copy into app:/tmp/picked/ (D4). A
    /// cancel finishes with no items. When an item does not stage, `done`
    /// has why, beside the files that did.
    static func stage(_ providers: [NSItemProvider], done: @escaping ([(url: URL, name: String)], String?) -> Void) {
        if providers.isEmpty { return done([], nil) }
        let group = DispatchGroup(), lock = NSLock()
        var staged = [(url: URL, name: String)?](repeating: nil, count: providers.count)
        var failures = [String?](repeating: nil, count: providers.count)
        for (i, provider) in providers.enumerated() {
            let label = provider.suggestedName ?? "item \(i + 1)"
            guard let type = provider.registeredTypeIdentifiers.first(where: {
                UTType($0).map { $0.conforms(to: .image) || $0.conforms(to: .movie) } ?? false
            }) else {
                failures[i] = "\(label) is neither an image nor a movie"
                continue
            }
            group.enter()
            provider.loadFileRepresentation(forTypeIdentifier: type) { url, error in
                defer { group.leave() }
                let outcome: Result<(url: URL, name: String), Error> = Result {
                    guard let url else { throw error ?? CocoaError(.fileReadUnknown) }
                    let stage = FileManager.default.temporaryDirectory.appendingPathComponent("exact-pick-\(UUID().uuidString).\(url.pathExtension)")
                    try FileManager.default.copyItem(at: url, to: stage)
                    return (stage, provider.suggestedName.map { $0 + "." + url.pathExtension } ?? url.lastPathComponent)
                }
                lock.lock(); defer { lock.unlock() }
                switch outcome {
                case .success(let file): staged[i] = file
                case .failure(let error): failures[i] = "\(label): \(error.localizedDescription)"
                }
            }
        }
        group.notify(queue: .main) {
            let failed = failures.compactMap { $0 }
            done(staged.compactMap { $0 }, failed.isEmpty ? nil : failed.joined(separator: "; "))
        }
    }
}
#endif
