import AppKit
import ApplicationServices

// LLP 1172.021: this executable is an accessibility client, never loaded by the app.
struct Refusal: Error { let reason: String }

func attribute(_ element: AXUIElement, _ name: String, errors: inout [String]) -> CFTypeRef? {
    var value: CFTypeRef?
    let result = AXUIElementCopyAttributeValue(element, name as CFString, &value)
    if result != .success && result != .attributeUnsupported && result != .noValue {
        errors.append("\(name): AX error \(result.rawValue)")
    }
    return value
}

func snapshot(_ pid: pid_t) -> [String: Any] {
    var errors: [String] = [], elements: [[String: Any]] = [], seen: Set<AXUIElement> = []
    var truncated = false
    var mainFrame: [Double]?
    func walk(_ node: AXUIElement, _ depth: Int) {
        guard !seen.contains(node) else { return }
        guard elements.count < 2000 && depth < 64 else { truncated = true; return }
        seen.insert(node)
        var row: [String: Any] = ["depth": depth]
        for (key, name) in [("identifier", "AXIdentifier"), ("type", "AXRole"), ("label", "AXTitle"),
                            ("description", "AXDescription"), ("value", "AXValue"),
                            ("enabled", "AXEnabled"), ("selected", "AXSelected"), ("focused", "AXFocused")] {
            let value = attribute(node, name, errors: &errors)
            if let string = value as? String, !string.isEmpty {
                row[key] = String(string.prefix(1000))
                if string.count > 1000 { truncated = true }
            } else if let number = value as? NSNumber { row[key] = number }
        }
        if row["label"] == nil { row["label"] = row["description"] }
        var point = CGPoint.zero, size = CGSize.zero
        if let p = attribute(node, "AXPosition", errors: &errors), CFGetTypeID(p) == AXValueGetTypeID(),
           let s = attribute(node, "AXSize", errors: &errors), CFGetTypeID(s) == AXValueGetTypeID(),
           AXValueGetValue(p as! AXValue, .cgPoint, &point), AXValueGetValue(s as! AXValue, .cgSize, &size) {
            row["frame"] = [point.x, point.y, size.width, size.height]
        }
        elements.append(row)
        for child in attribute(node, "AXChildren", errors: &errors) as? [AXUIElement] ?? [] { walk(child, depth + 1) }
    }
    if AXIsProcessTrusted() {
        let app = AXUIElementCreateApplication(pid)
        AXUIElementSetMessagingTimeout(app, 2)
        if let value = attribute(app, "AXMainWindow", errors: &errors), CFGetTypeID(value) == AXUIElementGetTypeID() {
            let window = value as! AXUIElement
            var point = CGPoint.zero, size = CGSize.zero
            if let p = attribute(window, "AXPosition", errors: &errors), CFGetTypeID(p) == AXValueGetTypeID(),
               let s = attribute(window, "AXSize", errors: &errors), CFGetTypeID(s) == AXValueGetTypeID(),
               AXValueGetValue(p as! AXValue, .cgPoint, &point), AXValueGetValue(s as! AXValue, .cgSize, &size) {
                mainFrame = [point.x, point.y, size.width, size.height]
            }
        }
        walk(app, 0)
    } else { errors.append("Accessibility permission is unavailable for the see helper") }
    let windows = (CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
        as? [[String: Any]] ?? []).filter { ($0[kCGWindowOwnerPID as String] as? Int) == Int(pid) }
        .compactMap { w -> [String: Any]? in
            guard let id = w[kCGWindowNumber as String] as? Int,
                  let bounds = w[kCGWindowBounds as String] as? [String: Any],
                  let rect = CGRect(dictionaryRepresentation: bounds as CFDictionary),
                  rect.width > 1, rect.height > 1 else { return nil }
            return ["id": id, "frame": [rect.minX, rect.minY, rect.width, rect.height],
                    "layer": w[kCGWindowLayer as String] ?? 0, "title": w[kCGWindowName as String] ?? ""]
        }
    return ["pid": Int(pid), "elements": elements, "errors": Array(Set(errors)).sorted(), "mainFrame": mainFrame as Any? ?? NSNull(),
            "truncated": truncated, "windows": windows,
            "frontmost": NSWorkspace.shared.frontmostApplication?.processIdentifier == pid,
            "screenCapture": CGPreflightScreenCaptureAccess()]
}

func input(_ pid: pid_t, _ numbers: [Double]) throws -> [String: Any] {
    guard numbers.count == 2 || numbers.count == 5, numbers.allSatisfy(\.isFinite),
          AXIsProcessTrusted(), CGPreflightPostEventAccess() else {
        throw Refusal(reason: "input requires finite coordinates and Accessibility/event-posting permission")
    }
    guard NSWorkspace.shared.frontmostApplication?.processIdentifier == pid else {
        throw Refusal(reason: "input refused: target process is not frontmost")
    }
    let start = CGPoint(x: numbers[0], y: numbers[1])
    // CGEvent follows the global hit test: refuse an occluding app before posting a down.
    var hit: AXUIElement?
    let error = AXUIElementCopyElementAtPosition(AXUIElementCreateSystemWide(), Float(start.x), Float(start.y), &hit)
    var hitPID: pid_t = 0
    guard error == .success, let hit, AXUIElementGetPid(hit, &hitPID) == .success, hitPID == pid else {
        throw Refusal(reason: "input refused: outside hit test does not belong to target PID")
    }
    let drag = numbers.count == 5
    let duration = drag ? numbers[4] / 1000 : 0.06
    guard duration > 0 && duration <= 2 else { throw Refusal(reason: "input duration must be 1–2000 ms") }
    let source = CGEventSource(stateID: .hidSystemState)
    func event(_ kind: CGEventType, _ p: CGPoint) throws {
        guard let e = CGEvent(mouseEventSource: source, mouseType: kind, mouseCursorPosition: p, mouseButton: .left) else {
            throw Refusal(reason: "CGEvent allocation failed")
        }
        e.setIntegerValueField(.mouseEventClickState, value: 1)
        e.post(tap: .cghidEventTap)
    }
    return try gesture(numbers, event: event)
}

func gesture(_ numbers: [Double], event: (CGEventType, CGPoint) throws -> Void,
             wait: (Double) -> Void = { Thread.sleep(forTimeInterval: $0) }) throws -> [String: Any] {
    let start = CGPoint(x: numbers[0], y: numbers[1])
    let drag = numbers.count == 5
    let duration = drag ? numbers[4] / 1000 : 0.06
    let began = Date().timeIntervalSince1970 * 1000
    let clock = ProcessInfo.processInfo.systemUptime
    var last = start
    try event(.mouseMoved, start)
    try event(.leftMouseDown, start)
    let steps = drag ? max(2, Int(ceil(duration * 60))) : 1
    do {
        for step in 1...steps {
            let progress = Double(step) / Double(steps)
            // The delay defines the gesture's trajectory, not an observation wait.
            let remaining = clock + duration * progress - ProcessInfo.processInfo.systemUptime
            if remaining > 0 { wait(remaining) }
            if drag {
                last = CGPoint(x: start.x + numbers[2] * progress, y: start.y + numbers[3] * progress)
                try event(.leftMouseDragged, last)
            }
        }
    } catch {
        try? event(.leftMouseUp, last)
        throw error
    }
    try event(.leftMouseUp, last)
    return ["delivery": "CGEvent/HID", "start": began, "end": Date().timeIntervalSince1970 * 1000,
            "from": [start.x, start.y], "to": [last.x, last.y], "moves": drag ? steps : 0]
}

do {
    let args = Array(CommandLine.arguments.dropFirst())
    guard args.count >= 2, let pid = Int32(args[1]), pid > 0 else { throw Refusal(reason: "usage: see-mac tree|activate|input PID [x y [dx dy ms]]") }
    let result: [String: Any]
    switch args[0] {
    case "tree": result = snapshot(pid)
    case "activate":
        guard let app = NSRunningApplication(processIdentifier: pid), app.activate(options: []) else { throw Refusal(reason: "could not activate target PID") }
        result = ["requested": true]
    case "input":
        let values = args.dropFirst(2).compactMap(Double.init)
        guard values.count == args.count - 2 else { throw Refusal(reason: "input coordinates must be numbers") }
        result = try input(pid, values)
    default: throw Refusal(reason: "unknown see helper operation")
    }
    print(String(data: try JSONSerialization.data(withJSONObject: result, options: [.sortedKeys]), encoding: .utf8)!)
} catch {
    let reason = (error as? Refusal)?.reason ?? String(describing: error)
    let data = try! JSONSerialization.data(withJSONObject: ["error": reason])
    print(String(data: data, encoding: .utf8)!)
    exit(2)
}
