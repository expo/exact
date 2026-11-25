import AppKit
import ApplicationServices
import ExactPackage

public final class DesktopAccessibility: ExactPackage {
    private let context: ExactPackageContext
    private static let askedKey = "exact.desktop-accessibility.asked"
    private var observer: NSObjectProtocol?
    private let lock = NSLock()
    private var stopped = false
    private var hasStopped: Bool { lock.withLock { stopped } }

    public init(context: ExactPackageContext) { self.context = context }

    private static func trusted(prompt: Bool = false) -> Bool {
        let key = kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String
        return AXIsProcessTrustedWithOptions([key: prompt] as CFDictionary)
    }

    private func status() -> [String: Any] {
        ["available": true, "state": Self.trusted() ? "granted" :
            UserDefaults.standard.bool(forKey: Self.askedKey) ? "denied" : "prompt"]
    }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?,
                      reply: @escaping ([String: Any]) -> Void) {
        switch name {
        case "accessibilityStatus":
            if observer == nil {
                observer = NotificationCenter.default.addObserver(forName: NSApplication.didBecomeActiveNotification,
                    object: nil, queue: .main) { [weak self] _ in self?.context.changed("desktop-accessibility.accessibilityStatus") }
            }
            reply(status())
        case "requestAccessibility":
            if !Self.trusted() {
                guard NSApp.isActive else { return reply(["error": "NotAllowedError"]) }
                UserDefaults.standard.set(true, forKey: Self.askedKey)
                _ = Self.trusted(prompt: true)
            }
            context.changed("desktop-accessibility.accessibilityStatus")
            reply(status())
        case "elementAtPoint":
            guard let x = Self.coordinate(args["x"]), let y = Self.coordinate(args["y"]) else { return reply(["error": "TypeError"]) }
            // The hit can belong to this process. Leave its main thread free to answer AX.
            DispatchQueue.global(qos: .userInitiated).async {
                let answer = self.hasStopped ? ["error": "AbortError"] : Self.hit(x, y)
                DispatchQueue.main.async {
                    if self.hasStopped { return reply(["error": "AbortError"]) }
                    if answer["error"] == nil && !Self.trusted() { return reply(Self.denied) }
                    reply(answer)
                }
            }
        default: reply(["error": "NotSupportedError"])
        }
    }

    public func release(_ name: String) {
        if name == "accessibilityStatus", let observer {
            NotificationCenter.default.removeObserver(observer)
            self.observer = nil
        }
    }

    public func destroy(_ done: @escaping () -> Void) {
        lock.withLock { stopped = true }
        release("accessibilityStatus")
        done()
    }

    private static func coordinate(_ value: Any?) -> Float? {
        guard let number = value as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID(),
              number.floatValue.isFinite else { return nil }
        return number.floatValue
    }

    private enum Failure: String, Error {
        case missing = "NotFoundError", failed = "OperationError", denied = "NotAllowedError"
    }

    private static var denied: [String: String] {
        UserDefaults.standard.bool(forKey: askedKey) ? ["error": "NotAllowedError"] :
            ["error": "NotAllowedError", "message": "requestAccessibility first"]
    }

    private static func check(_ status: AXError) throws {
        switch status {
        case .success: return
        case .apiDisabled: throw Failure.denied
        case .noValue, .attributeUnsupported, .invalidUIElement: throw Failure.missing
        default: throw Failure.failed
        }
    }

    private static func attribute(_ element: AXUIElement, _ key: String) throws -> CFTypeRef? {
        var result: CFTypeRef?
        let status = AXUIElementCopyAttributeValue(element, key as CFString, &result)
        if status == .attributeUnsupported || status == .noValue { return nil }
        try check(status)
        return result
    }

    private static func label(_ element: AXUIElement) throws -> String? {
        for key in [kAXTitleAttribute, kAXDescriptionAttribute] {
            if let text = try attribute(element, key) as? String {
                let label = text.trimmingCharacters(in: .whitespacesAndNewlines)
                if !label.isEmpty { return label }
            }
        }
        return nil
    }

    private static func name(_ element: AXUIElement) throws -> String {
        if let text = try label(element) { return text }
        guard let namedBy = try attribute(element, kAXTitleUIElementAttribute),
              CFGetTypeID(namedBy) == AXUIElementGetTypeID(), !CFEqual(namedBy, element) else { return "" }
        let title = namedBy as! AXUIElement
        if let text = try label(title) { return text }
        // A static label exposes its displayed text as AXValue. Only this explicit
        // label relation can supply it. The hit control's value is never read.
        guard try attribute(title, kAXRoleAttribute) as? String == kAXStaticTextRole else { return "" }
        let text = try attribute(title, kAXValueAttribute) as? String ?? ""
        return text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private static func unpack<T>(_ element: AXUIElement, _ key: String,
                                   _ type: AXValueType, _ result: inout T) throws -> Bool {
        guard let value = try attribute(element, key), CFGetTypeID(value) == AXValueGetTypeID() else { return false }
        return withUnsafeMutablePointer(to: &result) { AXValueGetValue(value as! AXValue, type, $0) }
    }

    private static func hit(_ x: Float, _ y: Float) -> [String: Any] {
        guard trusted() else { return denied }
        do {
            var hit: AXUIElement?
            try check(AXUIElementCopyElementAtPosition(AXUIElementCreateSystemWide(), x, y, &hit))
            guard let hit else { throw Failure.missing }
            var position = CGPoint.zero, size = CGSize.zero, pid: pid_t = 0
            try check(AXUIElementGetPid(hit, &pid))
            guard pid > 0, let role = try attribute(hit, kAXRoleAttribute) as? String,
                  try unpack(hit, kAXPositionAttribute, .cgPoint, &position),
                  try unpack(hit, kAXSizeAttribute, .cgSize, &size),
                  [position.x, position.y, size.width, size.height].allSatisfy(\.isFinite),
                  size.width >= 0, size.height >= 0 else { throw Failure.missing }
            let subrole = try attribute(hit, kAXSubroleAttribute) as? String ?? ""
            let title = subrole == kAXSecureTextFieldSubrole ? "" : try name(hit)
            let app = NSRunningApplication(processIdentifier: pid)
            var owner: [String: Any] = ["name": app?.localizedName ?? "", "pid": pid]
            if let id = app?.bundleIdentifier { owner["bundleId"] = id }
            return ["role": subroles[subrole] ?? roles[role] ?? "generic", "name": title, "app": owner,
                    "frame": ["x": position.x, "y": position.y, "width": size.width, "height": size.height]]
        } catch {
            let failure = error as? Failure ?? .failed
            return failure == .denied || !trusted() ? denied : ["error": failure.rawValue]
        }
    }

    private static let subroles = [
        kAXSecureTextFieldSubrole: "textbox", kAXSearchFieldSubrole: "searchbox",
        NSAccessibility.Subrole.tabButtonSubrole.rawValue: "tab", NSAccessibility.Subrole.textLink.rawValue: "link",
        kAXToggleSubrole: "button", kAXOutlineRowSubrole: "treeitem", kAXSwitchSubrole: "switch",
        kAXSortButtonSubrole: "button", kAXToolbarButtonSubrole: "button",
        kAXDialogSubrole: "dialog", kAXSystemDialogSubrole: "dialog",
        "AXApplicationStatus": "status", "AXApplicationAlert": "alert", "AXApplicationDialog": "dialog",
        "AXApplicationAlertDialog": "alertdialog", "AXLandmarkRegion": "region",
    ]

    private static let roles = [
        kAXApplicationRole: "application", kAXButtonRole: "button", kAXMenuButtonRole: "button",
        kAXPopUpButtonRole: "combobox", kAXDisclosureTriangleRole: "button",
        NSAccessibility.Role.link.rawValue: "link", kAXRadioButtonRole: "radio", kAXCheckBoxRole: "checkbox",
        kAXTabGroupRole: "tablist", kAXTableRole: "table", kAXRowRole: "row", kAXOutlineRole: "tree",
        kAXScrollBarRole: "scrollbar", kAXRadioGroupRole: "radiogroup", kAXListRole: "list",
        kAXGroupRole: "group", kAXComboBoxRole: "combobox", kAXSliderRole: "slider",
        kAXBusyIndicatorRole: "progressbar", kAXIncrementorRole: "spinbutton",
        kAXProgressIndicatorRole: "progressbar", kAXToolbarRole: "toolbar",
        kAXTextFieldRole: "textbox", kAXTextAreaRole: "textbox", kAXHeadingRole: "heading",
        kAXMenuBarRole: "menubar", kAXMenuBarItemRole: "menuitem", kAXMenuRole: "menu", kAXMenuItemRole: "menuitem",
        kAXSplitterRole: "separator", kAXImageRole: "img", kAXHelpTagRole: "tooltip", kAXGridRole: "grid",
        kAXRelevanceIndicatorRole: "meter", kAXLevelIndicatorRole: "meter", kAXCellRole: "cell", kAXSheetRole: "dialog",
    ]
}
