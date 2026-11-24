#if os(iOS)
import ObjectiveC
import UIKit

/// The keyboard as its window's guides give it (LLP 1123 D7): a guide that follows an undocked keyboard gives its
/// frame, docked or not, and one that does not has height only while it is docked. Each is the guide of a child that
/// fills the window, with the bottom safe area left out, so it gives the keyboard's own frame. A window's guides follow
/// an interactive dismissal before UIKit's notifications arrive, and a scaled sheet's own guide can end short of the
/// keyboard, so each owner reads this sample converted into its own view (`OwnerUpdateIOS`). It is made with the first
/// keyboard editor: a constraint gives its window a layout engine, which every view added later joins.
package final class KeyboardSampleIOS {
    /// The keyboard's frame in its window, and whether UIKit docks it.
    package struct Sample: Equatable {
        package var frame: CGRect
        package var docked: Bool
    }

    private let following = Probe(followsUndocked: true), docked = Probe(followsUndocked: false)

    nonisolated(unsafe) private static var key: UInt8 = 0

    /// `window`'s sampler, if it has one.
    package static func existing(in window: UIWindow) -> KeyboardSampleIOS? {
        objc_getAssociatedObject(window, &key) as? KeyboardSampleIOS
    }

    /// `window`'s sampler, made and put behind its other views on first use.
    @discardableResult
    package static func install(in window: UIWindow) -> KeyboardSampleIOS {
        if let sampler = existing(in: window) { return sampler }
        let sampler = KeyboardSampleIOS()
        for probe in [sampler.docked, sampler.following] {
            probe.frame = window.bounds
            probe.autoresizingMask = [.flexibleWidth, .flexibleHeight]
            window.insertSubview(probe, at: 0)
        }
        objc_setAssociatedObject(window, &key, sampler, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        return sampler
    }

    /// The keyboard now, `nil` while none shows or the sampler is out of its window.
    package var sample: Sample? {
        guard let window = following.window else { return nil }
        let frame = following.keyboardLayoutGuide.layoutFrame
        guard frame.height > 0 else { return nil }
        return Sample(frame: following.convert(frame, to: window), docked: docked.keyboardLayoutGuide.layoutFrame.height > 0)
    }

    /// A clear child the size of its window that takes no touches and no accessibility, whose zero-size dependent
    /// keeps its guide in the window's layout, so the guide's frame is UIKit's as it moves.
    private final class Probe: UIView {
        init(followsUndocked: Bool) {
            super.init(frame: .zero)
            isUserInteractionEnabled = false
            accessibilityElementsHidden = true
            keyboardLayoutGuide.usesBottomSafeArea = false
            keyboardLayoutGuide.followsUndockedKeyboard = followsUndocked
            let dependent = UIView()
            dependent.isHidden = true
            dependent.translatesAutoresizingMaskIntoConstraints = false
            addSubview(dependent)
            NSLayoutConstraint.activate([
                dependent.topAnchor.constraint(equalTo: keyboardLayoutGuide.topAnchor),
                dependent.leadingAnchor.constraint(equalTo: keyboardLayoutGuide.leadingAnchor),
                dependent.widthAnchor.constraint(equalToConstant: 0),
                dependent.heightAnchor.constraint(equalToConstant: 0),
            ])
        }

        required init?(coder: NSCoder) { nil }
    }
}
#endif
