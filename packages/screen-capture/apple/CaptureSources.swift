#if os(macOS)
import AppKit
import ExactPackage
import ScreenCaptureKit

struct CaptureScreen {
    let id: CGDirectDisplayID
    let record: [String: Any]
    var width: Int
    var height: Int
    var index = 0
    var pointer: CGPoint?
    var crop: CGRect?
    var windowFrame: CGRect?
    var filter: SCContentFilter?
    var windowID: CGWindowID?

    func verifyWindow() throws {
        guard let windowID, let windowFrame else { return }
        let list = CGWindowListCopyWindowInfo(.optionIncludingWindow, windowID) as? [[String: Any]]
        guard let info = list?.first, let raw = info[kCGWindowBounds as String] as? NSDictionary,
              let current = CGRect(dictionaryRepresentation: raw as CFDictionary) else { throw missing }
        guard current == windowFrame else { throw CaptureFailure("EncodingError", "window moved during capture") }
    }

    var bounds: CGRect {
        CGRect(x: (record["left"] as? NSNumber)?.doubleValue ?? 0, y: (record["top"] as? NSNumber)?.doubleValue ?? 0,
               width: (record["width"] as? NSNumber)?.doubleValue ?? 0, height: (record["height"] as? NSNumber)?.doubleValue ?? 0)
    }

    static func ordered(_ a: Self, _ b: Self) -> Bool {
        (a.bounds.minX, a.bounds.minY) < (b.bounds.minX, b.bounds.minY)
    }
}

extension CGRect {
    var captureRecord: [String: Double] { ["x": origin.x, "y": origin.y, "width": width, "height": height] }
}

private let missing = CaptureFailure("NotFoundError", "capture source is no longer available")
private let invalid = CaptureFailure("TypeError", "choose a window or a display with an optional in-bounds rect")

private struct WindowKey: Hashable {
    let id: CGWindowID
    let pid: pid_t
}

final class CaptureSources {
    // The OS id never crosses the package boundary. Drop handles whose source
    // disappears from a fresh snapshot, even if the OS later recycles the id.
    private var handles: [WindowKey: String] = [:]
    private let fixtureHandle = UUID().uuidString

    private func windows(_ content: SCShareableContent) -> [CGWindowID: SCWindow] {
        let list = content.windows.filter { $0.owningApplication != nil && !$0.frame.isEmpty }
        let keys = Set(list.map { WindowKey(id: $0.windowID, pid: $0.owningApplication!.processID) })
        handles = handles.filter { keys.contains($0.key) }
        return Dictionary(list.map { ($0.windowID, $0) }, uniquingKeysWith: { first, _ in first })
    }

    private func fixtureScreen(_ screens: [CaptureScreen]) -> CaptureScreen? {
        screens.first { $0.record["isPrimary"] as? Bool == true } ?? screens.first
    }

    // AppKit returns the window that would receive this mouse-down, including
    // another app's panel, and skips windows that pass clicks through.
    func hit(_ point: CGPoint?) -> CGWindowID? {
        if let point {
            let native = NSPoint(x: point.x, y: ExactScreenGeometry.primaryTop - point.y)
            return CGWindowID(exactly: NSWindow.windowNumber(at: native, belowWindowWithWindowNumber: 0))
        }
        guard let front = NSWorkspace.shared.frontmostApplication?.processIdentifier else { return nil }
        let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
            as? [[String: Any]] ?? []
        return list.first { ($0[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == front }?[kCGWindowNumber as String] as? CGWindowID
    }

    func lookup(point: CGPoint?, hit: CGWindowID?, screens: [CaptureScreen], content: SCShareableContent?) throws -> [String: Any] {
        guard let content else {
            guard let screen = fixtureScreen(screens), point.map(screen.bounds.contains) ?? true else { throw missing }
            return ["handle":fixtureHandle, "title":"Agent window", "frame":screen.bounds.captureRecord,
                    "app":["name":"Agent", "bundleId":"exact.agent", "pid":1]]
        }
        let candidates = windows(content)
        guard let hit, let window = candidates[hit], let owner = window.owningApplication else { throw missing }
        let key = WindowKey(id: window.windowID, pid: owner.processID)
        let handle = handles[key] ?? UUID().uuidString
        handles[key] = handle
        return ["handle":handle, "title":window.title ?? "", "frame":window.frame.captureRecord,
                "app":["name":owner.applicationName, "bundleId":owner.bundleIdentifier, "pid":owner.processID]]
    }

    func targets(_ screens: [CaptureScreen], args: [String: Any], content: SCShareableContent?) throws -> [CaptureScreen] {
        let screen = args["screen"] as? [String: Any], rect = args["rect"] as? [String: Any]
        let window = args["window"] as? String
        func given(_ name: String) -> Bool { args[name] != nil && !(args[name] is NSNull) }
        guard (!given("window") || window != nil), (!given("screen") || screen != nil),
              (!given("rect") || rect != nil), (!given("cursor") || ["always","never"].contains(args["cursor"] as? String ?? "")) else { throw invalid }
        guard rect == nil || screen != nil, screen == nil || window == nil else { throw invalid }
        if let window {
            guard let content else {
                guard window == fixtureHandle, var target = fixtureScreen(screens) else { throw missing }
                target.pointer = CGPoint(x: target.width / 2, y: target.height / 2)
                target.windowFrame = target.bounds
                return [target]
            }
            let candidates = windows(content)
            guard let key = handles.first(where: { $0.value == window })?.key,
                  let native = candidates[key.id], native.owningApplication?.processID == key.pid else { throw missing }
            let frame = native.frame
            func overlap(_ s: CaptureScreen) -> CGFloat {
                let common = frame.intersection(s.bounds)
                return common.isNull ? 0 : common.width * common.height
            }
            guard var target = screens.max(by: { overlap($0) < overlap($1) }), overlap(target) > 0 else { throw missing }
            let filter = SCContentFilter(desktopIndependentWindow: native)
            let scale = CGFloat(filter.pointPixelScale)
            target.width = try dimension(frame.width * scale)
            target.height = try dimension(frame.height * scale)
            target.windowFrame = frame
            target.windowID = native.windowID
            target.filter = filter
            return [target]
        }
        var selected = screens
        if let screen {
            let fields = ["left", "top", "width", "height", "devicePixelRatio", "label"]
            selected = screens.filter { target in fields.allSatisfy { field in
                (screen[field] as? NSObject) == (target.record[field] as? NSObject)
            } }
            guard selected.count == 1 else { throw missing }
        }
        return try selected.map { original in
            var target = original
            if let rect {
                guard let x = rect["x"] as? Double, let y = rect["y"] as? Double,
                      let w = rect["width"] as? Double, let h = rect["height"] as? Double,
                      x >= 0, y >= 0, w > 0, h > 0, x + w <= target.bounds.width,
                      y + h <= target.bounds.height else { throw invalid }
                target.crop = CGRect(x: x, y: y, width: w, height: h)
                target.width = try dimension(w * Double(target.width) / target.bounds.width)
                target.height = try dimension(h * Double(target.height) / target.bounds.height)
            }
            if content == nil, original.index == fixtureScreen(screens)?.index {
                let region = target.crop ?? CGRect(origin: .zero, size: target.bounds.size)
                let centre = CGPoint(x: target.bounds.width / 2, y: target.bounds.height / 2)
                if region.contains(centre) {
                    target.pointer = CGPoint(x: (centre.x - region.minX) * Double(target.width) / region.width,
                                             y: (centre.y - region.minY) * Double(target.height) / region.height)
                }
            }
            if let content {
                guard let display = content.displays.first(where: { $0.displayID == target.id }) else {
                    throw CaptureFailure("EncodingError", "display disconnected before capture")
                }
                target.filter = SCContentFilter(display: display, excludingWindows: [])
            }
            return target
        }
    }

    private func dimension(_ value: Double) throws -> Int {
        guard value.isFinite, value > 0, value <= Double(Int32.max) else {
            throw CaptureFailure("QuotaExceededError", "capture exceeds the pixel limit")
        }
        return max(1, Int(value.rounded()))
    }
}
#endif
