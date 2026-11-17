// @ref LLP 1133.024 D4 and D6, amended 2026-11-15 — the share extension with
// no editor (`share_target.apple.presentation: "confirmation"`): choosing the
// app in the share sheet writes the inbox (ShareInbox.swift), shows the app's
// name under a checkmark for a moment, and ends. The bake compiles this file
// in place of ShareViewController.swift, so an extension holds one
// presentation.
import Foundation
import UniformTypeIdentifiers

/// A confirmation's save. Its controller withdraws it when the sheet goes away before the share is whole.
final class ShareSave {
    fileprivate var writer: ShareInboxWriter?
    fileprivate(set) var withdrawn = false
    /// Whether the save is over: the record is written, or nothing of the share is left in the inbox.
    fileprivate(set) var ended = false

    /// Withdraws a save that has not ended: no record is written, `done` does not run, and what was copied is
    /// removed, now and again once the copies in flight have ended. Called on the main queue, where the save's
    /// steps run.
    func withdraw() {
        guard !ended else { return }
        withdrawn = true
        writer?.discard()
    }
}

extension ShareInboxWriter {
    /// How long the checkmark stays before the request completes.
    static let confirmation: TimeInterval = 1
    /// The largest text an attachment may carry, in bytes of its file representation. A string is built whole in
    /// the extension's memory.
    static let textCap = 1 << 20

    /// Writes `items` as one share, as the compose sheet's Post does with its text unedited. `done` runs on the
    /// main queue, in a later turn, with the error when nothing was written, which leaves nothing in the inbox.
    /// Text is loaded only for an app that maps `text`, so an attachment the record would leave out cannot fail
    /// the share.
    static func save(_ items: [NSExtensionItem], info: ShareTargetInfo?, container: URL?, done: @escaping (Error?) -> Void) -> ShareSave {
        let save = ShareSave()
        let end = { (error: Error?) in
            save.ended = true
            if !save.withdrawn { done(error) }
        }
        guard let info, let container = container ?? ShareInboxWriter.container(info),
              let writer = ShareInboxWriter(info: info, container: container) else {
            DispatchQueue.main.async { end(NSError(domain: "exact.share", code: 1, userInfo: [NSLocalizedDescriptionKey: "The app's shared container is not available."])) }
            return save
        }
        save.writer = writer
        let write = { (text: Result<String?, Error>) in
            do {
                // A withdrawn save fails as a cancel, and the copies that ended since are removed with it.
                if save.withdrawn { throw CocoaError(.userCancelled) }
                try writer.finish(title: items.first?.attributedTitle?.string, text: text.get())
                end(nil)
            } catch {
                writer.discard()
                end(error)
            }
        }
        writer.take(items) {
            if info.mapped.contains("text"), !save.withdrawn { text(of: items, done: write) } else { write(.success(nil)) }
        }
        return save
    }

    /// The share's text: the item's own, or, from a sender that gives none, what its first plain-text attachment
    /// holds. That is the attachment `take` leaves for the sheet's text, by the same test. A share with no such
    /// attachment has no text; one whose attachment does not load, or is over `textCap`, is a failure, since the
    /// share would be saved without what the person sent. The text is decoded from the attachment's file, read up
    /// to the cap, as `NSString` reads an item provider's data of that type; it is never loaded as an object,
    /// which a provider may answer with another representation of any size. `done` runs on the main queue.
    static func text(of items: [NSExtensionItem], done: @escaping (Result<String?, Error>) -> Void) {
        if let text = items.first?.attributedContentText?.string, !text.isEmpty { return done(.success(text)) }
        let carrier = items.lazy.flatMap { $0.attachments ?? [] }.first { provider in
            provider.hasItemConformingToTypeIdentifier(UTType.plainText.identifier) && provider.suggestedName == nil
                && ![UTType.url, .fileURL].contains { provider.hasItemConformingToTypeIdentifier($0.identifier) }
        }
        guard let carrier else { return done(.success(nil)) }
        let unread = NSError(domain: "exact.share", code: 2, userInfo: [NSLocalizedDescriptionKey: "The shared text could not be read."])
        let type = carrier.registeredTypeIdentifiers.first { UTType($0)?.conforms(to: .plainText) == true } ?? UTType.plainText.identifier
        _ = carrier.loadFileRepresentation(forTypeIdentifier: type) { file, error in
            let text: Result<String?, Error>
            if let data = head(of: file, cap: textCap) {
                if data.count > textCap {
                    text = .failure(NSError(domain: "exact.share", code: 5, userInfo: [NSLocalizedDescriptionKey: "The shared text is too long to save."]))
                } else {
                    text = Result { try NSString.object(withItemProviderData: data, typeIdentifier: type) as String }
                }
            } else {
                text = .failure(error ?? unread)
            }
            DispatchQueue.main.async { done(text) }
        }
    }
}

#if canImport(UIKit)
import UIKit

@objc(ExactShareConfirmationViewController)
public final class ExactShareConfirmationViewController: UIViewController {
    /// The inbox's target and its container (the App Group's when nil), the extension's context and the app's name;
    /// a test sets all four.
    public var info: ShareTargetInfo? = ShareTargetInfo(bundle: .main)
    public var container: URL?
    public var stubContext: NSExtensionContext?
    private var context: NSExtensionContext? { stubContext ?? extensionContext }
    public var name = Bundle.main.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String
    /// Posts an accessibility notification; a test records what is posted.
    public var postAccessibility: (UIAccessibility.Notification, Any?) -> Void = { UIAccessibility.post(notification: $0, argument: $1) }
    private let label = UILabel()
    private let content = UIStackView()
    private let progress = UIActivityIndicatorView(style: .large)
    private let mark = UIImageView(image: UIImage(systemName: "checkmark", withConfiguration: UIImage.SymbolConfiguration(textStyle: .largeTitle, scale: .large)))
    private var save: ShareSave?
    private var failure: Error?
    private var requestEnded = false
    /// Whether the checkmark is showing: the share is in the inbox.
    public var confirmed: Bool { !mark.isHidden }
    /// Whether the save is over, written or not; a test waits for it.
    public var saveEnded: Bool { save?.ended ?? false }

    public override func loadView() {
        label.text = name
        label.font = .preferredFont(forTextStyle: .headline)
        label.adjustsFontForContentSizeCategory = true
        label.textAlignment = .center
        label.numberOfLines = 0
        mark.tintColor = .label
        mark.isHidden = true
        progress.startAnimating()
        [progress, mark, label].forEach(content.addArrangedSubview)
        content.axis = .vertical
        content.alignment = .center
        content.spacing = 12
        content.translatesAutoresizingMaskIntoConstraints = false
        // iOS presents the extension as a sheet. The root has no background of its own, so the sheet's shows.
        let root = UIView()
        root.addSubview(content)
        NSLayoutConstraint.activate([
            content.centerXAnchor.constraint(equalTo: root.centerXAnchor), content.centerYAnchor.constraint(equalTo: root.centerYAnchor),
            content.widthAnchor.constraint(lessThanOrEqualTo: root.layoutMarginsGuide.widthAnchor),
        ])
        view = root
    }

    public override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        guard save == nil else { return }
        save = ShareInboxWriter.save(context?.inputItems as? [NSExtensionItem] ?? [], info: info, container: container) { [weak self] error in
            guard let self else { return }
            self.progress.stopAnimating()
            if let error { return self.fail(error) }
            self.mark.isHidden = false
            UINotificationFeedbackGenerator().notificationOccurred(.success)
            self.postAccessibility(.layoutChanged, self.label)
            DispatchQueue.main.asyncAfter(deadline: .now() + ShareInboxWriter.confirmation) { self.end(nil) }
        }
    }

    /// The sheet is gone. If the person dismissed it before the share was whole, nothing is kept and the request
    /// is cancelled. Under the checkmark the share is already in the inbox, so the request completes.
    public override func viewDidDisappear(_ animated: Bool) {
        super.viewDidDisappear(animated)
        save?.withdraw()
        end(confirmed ? nil : failure ?? CocoaError(.userCancelled))
    }

    /// Ends the request, once: completed, or cancelled with `error`.
    private func end(_ error: Error?) {
        guard !requestEnded else { return }
        requestEnded = true
        if let error { context?.cancelRequest(withError: error) } else { context?.completeRequest(returningItems: [], completionHandler: nil) }
    }

    /// Nothing was saved: an alert says why, and its button cancels the request.
    private func fail(_ error: Error) {
        failure = error
        content.isHidden = true
        guard viewIfLoaded?.window != nil else { return end(error) }
        let alert = UIAlertController(title: name, message: error.localizedDescription, preferredStyle: .alert)
        alert.addAction(UIAlertAction(title: NSLocalizedString("OK", comment: ""), style: .default) { [weak self] _ in self?.end(error) })
        present(alert, animated: true)
    }
}
#elseif canImport(AppKit)
import AppKit

@objc(ExactShareConfirmationViewController)
public final class ExactShareConfirmationViewController: NSViewController {
    /// The inbox's target and its container (the App Group's when nil), the extension's context and the app's name;
    /// a test sets all four.
    public var info: ShareTargetInfo? = ShareTargetInfo(bundle: .main)
    public var container: URL?
    public var stubContext: NSExtensionContext?
    private var context: NSExtensionContext? { stubContext ?? extensionContext }
    public var name = Bundle.main.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String ?? ""
    /// Posts an accessibility notification; a test records what is posted.
    public var postAccessibility: (Any, NSAccessibility.Notification, [NSAccessibility.NotificationUserInfoKey: Any]) -> Void = {
        NSAccessibility.post(element: $0, notification: $1, userInfo: $2)
    }
    private let progress = NSProgressIndicator()
    private let mark = NSImageView()
    private var save: ShareSave?
    private var failure: Error?
    private var requestEnded = false
    /// Whether the checkmark is showing: the share is in the inbox.
    public var confirmed: Bool { !mark.isHidden }
    /// Whether the save is over, written or not; a test waits for it.
    public var saveEnded: Bool { save?.ended ?? false }

    public override func loadView() {
        progress.style = .spinning
        progress.startAnimation(nil)
        mark.image = NSImage(systemSymbolName: "checkmark", accessibilityDescription: nil)
        mark.symbolConfiguration = NSImage.SymbolConfiguration(textStyle: .largeTitle)
        mark.contentTintColor = .labelColor
        mark.isHidden = true
        // The checkmark is decorative. An image view's accessibility element is its cell, which AppKit would read
        // by the symbol's own description, "selected".
        mark.cell?.setAccessibilityElement(false)
        let label = NSTextField(labelWithString: name)
        label.font = .preferredFont(forTextStyle: .headline)
        let stack = NSStackView(views: [progress, mark, label])
        stack.orientation = .vertical
        stack.alignment = .centerX
        stack.spacing = 12
        stack.edgeInsets = NSEdgeInsets(top: 24, left: 24, bottom: 24, right: 24)
        stack.widthAnchor.constraint(greaterThanOrEqualToConstant: 200).isActive = true
        view = stack
    }

    public override func viewDidAppear() {
        super.viewDidAppear()
        guard save == nil else { return }
        save = ShareInboxWriter.save(context?.inputItems as? [NSExtensionItem] ?? [], info: info, container: container) { [weak self] error in
            guard let self else { return }
            self.progress.stopAnimation(nil)
            self.progress.isHidden = true
            if let error { return self.fail(error) }
            self.mark.isHidden = false
            // An announcement is posted for the application element (NSAccessibilityConstants.h,
            // `NSAccessibilityAnnouncementRequestedNotification`).
            self.postAccessibility(NSApplication.shared, .announcementRequested, [.announcement: self.name, .priority: NSAccessibilityPriorityLevel.high.rawValue])
            DispatchQueue.main.asyncAfter(deadline: .now() + ShareInboxWriter.confirmation) { self.end(nil) }
        }
    }

    /// The sheet is gone. If the person dismissed it before the share was whole, nothing is kept and the request
    /// is cancelled. Under the checkmark the share is already in the inbox, so the request completes.
    public override func viewDidDisappear() {
        super.viewDidDisappear()
        save?.withdraw()
        end(confirmed ? nil : failure ?? CocoaError(.userCancelled))
    }

    /// Ends the request, once: completed, or cancelled with `error`.
    private func end(_ error: Error?) {
        guard !requestEnded else { return }
        requestEnded = true
        if let error { context?.cancelRequest(withError: error) } else { context?.completeRequest(returningItems: [], completionHandler: nil) }
    }

    /// Nothing was saved: an alert says why, and its button cancels the request.
    private func fail(_ error: Error) {
        failure = error
        guard let window = viewIfLoaded?.window else { return end(error) }
        NSAlert(error: error).beginSheetModal(for: window) { [weak self] _ in self?.end(error) }
    }
}
#endif
