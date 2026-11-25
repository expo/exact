#if os(macOS)
import AppKit

/// `EXACT_AGENT_DISPLAY` (`agent.mjs --display`): the display a driven Mac
/// app's windows open on (LLP 1012, "where a driven Mac window opens"). A
/// live drive's window has to be uncovered to run as a person's launch does,
/// so a drive on a Mac someone is using names a display they are not using.
public enum AgentDisplay {
    /// The display a drive named, or `nil` when it named none. A name that
    /// matches no display, or more than one, ends the launch with the list
    /// before any window shows: a drive never opens on a display it did not ask for.
    public static let named: NSScreen? = {
        guard ExactEnv.agentCarried, let name = ExactEnv.environment["EXACT_AGENT_DISPLAY"] else { return nil }
        let screens = NSScreen.screens
        switch pick(name, among: screens.map(\.localizedName)) {
        case .success(let index): return screens[index]
        case .failure(let refusal):
            fputs("exact: \(refusal)\n", stderr)
            exit(2)
        }
    }()

    /// Whether `window` is on the named display: `NSScreen` objects for one
    /// display need not be the same object, its `NSScreenNumber` is.
    public static func holds(_ window: NSWindow) -> Bool {
        let number = NSDeviceDescriptionKey("NSScreenNumber")
        guard let named, let screen = window.screen else { return false }
        return (screen.deviceDescription[number] as? NSNumber) == (named.deviceDescription[number] as? NSNumber)
    }

    /// `window` centred on the named display, moved by `offset` (a drive's
    /// spread), and kept on it.
    public static func place(_ window: NSWindow, offset: NSPoint = .zero) {
        guard let named else { return }
        window.setFrame(frame(window.frame.size, centredIn: named.visibleFrame, offset: offset), display: false)
    }

    /// The display `name` picks among `names` (`NSScreen.screens`' names, in
    /// its order): a whole number is an index; anything else is a
    /// `localizedName`, compared without case, that only one display has.
    public static func pick(_ name: String, among names: [String]) -> Result<Int, Refusal> {
        let wanted = name.trimmingCharacters(in: .whitespaces)
        if let index = Int(wanted), wanted.allSatisfy(\.isNumber) {
            return names.indices.contains(index) ? .success(index) : .failure(Refusal(name: name, names: names, ambiguous: false))
        }
        let matches = wanted.isEmpty ? [] : names.indices.filter { names[$0].compare(wanted, options: .caseInsensitive) == .orderedSame }
        return matches.count == 1 ? .success(matches[0]) : .failure(Refusal(name: name, names: names, ambiguous: matches.count > 1))
    }

    public struct Refusal: Error, CustomStringConvertible {
        let name: String, names: [String], ambiguous: Bool
        public var description: String {
            let list = names.enumerated().map { "\($0.offset) \"\($0.element)\"" }.joined(separator: ", ")
            return "EXACT_AGENT_DISPLAY \"\(name)\" \(ambiguous ? "names more than one display; name it by index" : "names no display here"): \(list.isEmpty ? "there is none" : list)"
        }
    }

    /// A window frame of `size` centred in a display's `visibleFrame` and
    /// moved by `offset`, its top left corner, title bar included, kept on it.
    public static func frame(_ size: NSSize, centredIn visible: NSRect, offset: NSPoint = .zero) -> NSRect {
        let x = visible.minX + (visible.width - size.width) / 2 + offset.x
        let top = visible.maxY - (visible.height - size.height) / 2 + offset.y
        let left = max(visible.minX, min(x, visible.maxX - size.width))
        let kept = min(visible.maxY, max(top, visible.minY + size.height))
        return NSRect(x: left.rounded(), y: (kept - size.height).rounded(), width: size.width, height: size.height)
    }
}
#endif
