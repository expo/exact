// @exact/global-shortcuts on macOS (LLP 1133.025 D3): a chord registered
// system-wide with Carbon's `RegisterEventHotKey` while something reads it,
// and modifier shortcuts heard by a `.flagsChanged` monitor.
// The app's composed module calls it outside the agent; under the agent the
// runner's substitute answers, unless the drive binds it.
//
// One chord, many readers: the process registers a chord once and counts
// its readers, each an instance (a session) and an `id`. A press goes to
// the readers of one session: the key window's if it reads the chord, else
// the oldest reader's (the status session is the host's to know, so this
// module does not prefer it). The RFC puts the one hot-key handler in the
// host's package context (D11 item 2); it lives here, process-wide, until a
// second package needs it.
import AppKit
import ApplicationServices
import Carbon.HIToolbox
import ExactPackage
import IOKit.hidsystem

public final class GlobalShortcutsModule: ExactPackage {
    private let context: ExactPackageContext
    /// The chords this session reads, each with its `id`: a reader that
    /// moves to new keys reads both until the runner releases the old ones.
    private var reading: Set<Reading> = []
    private struct Reading: Hashable { let id: String; let keys: String }

    public init(context: ExactPackageContext) { self.context = context }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        DispatchQueue.main.async { [self] in
            let keys = args["keys"] as? String ?? ""
            switch name {
            case "globalShortcut":
                let id = args["id"] as? String ?? ""
                let (registered, reason) = HotKeys.shared.read(keys, id: id, by: self)
                if registered { reading.insert(Reading(id: id, keys: keys)) }
                context.log("global-shortcuts: \(keys) for \(id): \(registered ? "registered" : "not registered (\(reason ?? ""))")")
                reply(["id": id, "keys": keys, "registered": registered, "reason": reason ?? NSNull()])
            case "checkGlobalShortcut":
                let (registered, reason) = HotKeys.shared.check(keys)
                reply(["keys": keys, "registered": registered, "reason": reason ?? NSNull()])
            case "globalShortcutStatus":
                reply(status())
            default: reply(["error": "NotSupportedError"])
            }
        }
    }

    public func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] {
        name == "globalShortcutStatusSync" ? status() : ["error": "NotSupportedError"]
    }

    private func status() -> [String: Any] {
        ["available": true, "releaseEvents": true, "apple": ["accessibilityTrusted": AXIsProcessTrusted()]]
    }

    /// No reader asks with these arguments any longer: this session's
    /// reading of that chord for that id ends.
    public func release(_ name: String, args: [String: Any]) {
        guard name == "globalShortcut", let id = args["id"] as? String, let keys = args["keys"] as? String else { return }
        DispatchQueue.main.async { [self] in
            guard reading.remove(Reading(id: id, keys: keys)) != nil else { return }
            HotKeys.shared.release(keys, id: id, by: self)
            context.log("global-shortcuts: \(keys) for \(id): released")
        }
    }

    /// No reader of `globalShortcut` is left in this session.
    public func release(_ name: String) {
        guard name == "globalShortcut" else { return }
        DispatchQueue.main.async { [self] in releaseAll() }
    }

    private func releaseAll() {
        for read in reading { HotKeys.shared.release(read.keys, id: read.id, by: self) }
        reading = []
    }

    public func destroy(_ done: @escaping () -> Void) {
        DispatchQueue.main.async { [self] in
            releaseAll()
            done()
        }
    }

    fileprivate func shortcut(_ event: String, keys: String, id: String) {
        context.emit(event, [id, ["id": id, "keys": keys]])
    }

    /// Whether this session's window is the key window.
    fileprivate var isKey: Bool { context.presentation?.window.isKeyWindow == true }
}

/// The process's hot keys: one Carbon registration per chord, one monitor
/// shared by the modifier shortcuts, and the readers of each.
private final class HotKeys {
    static let shared = HotKeys()

    private struct Reader { weak var module: GlobalShortcutsModule?; let id: String }
    private final class Chord {
        var ref: EventHotKeyRef?
        let number: UInt32
        var readers: [Reader] = []
        // A focus change during a hold must restore the window that received the press.
        var pressed: [Reader]?
        init(number: UInt32) { self.number = number }
    }
    private var chords: [String: Chord] = [:]
    private var next: UInt32 = 1
    private var handler: EventHandlerRef?
    private var monitors: [Any] = []
    private var lastFlagsEvent: NSEvent?
    private static let signature: OSType = 0x4558_4354 // 'EXCT'

    /// `keys` read for `id` by `module`: registered, or why not.
    func read(_ keys: String, id: String, by module: GlobalShortcutsModule) -> (Bool, String?) {
        if let chord = chords[keys] {
            if !chord.readers.contains(where: { $0.module === module && $0.id == id }) { add(chord, id: id, by: module) }
            return (true, nil)
        }
        let number = next
        next += 1
        let chord = Chord(number: number)
        if Self.modifier(keys) != nil {
            guard AXIsProcessTrusted() else { return (false, "not-allowed") }
            guard installMonitors() else { return (false, "unsupported") }
        } else {
            let (ref, reason) = register(keys, number: number)
            guard let ref else { return (false, reason) }
            guard installHandler() else {
                UnregisterEventHotKey(ref)
                return (false, "unsupported")
            }
            chord.ref = ref
        }
        chords[keys] = chord
        add(chord, id: id, by: module)
        return (true, nil)
    }

    private func add(_ chord: Chord, id: String, by module: GlobalShortcutsModule) {
        chord.readers.append(Reader(module: module, id: id))
    }

    /// Whether `keys` could be registered now, keeping nothing.
    func check(_ keys: String) -> (Bool, String?) {
        if chords[keys] != nil { return (true, nil) }
        if Self.modifier(keys) != nil {
            guard AXIsProcessTrusted() else { return (false, "not-allowed") }
            let installed = !monitors.isEmpty
            guard installMonitors() else { return (false, "unsupported") }
            if !installed { removeMonitors() }
            return (true, nil)
        }
        let (ref, reason) = register(keys, number: 0)
        if let ref {
            UnregisterEventHotKey(ref)
            return installHandler() ? (true, nil) : (false, "unsupported")
        }
        return (false, reason)
    }

    func release(_ keys: String, id: String, by module: GlobalShortcutsModule) {
        guard let chord = chords[keys] else { return }
        if let held = chord.pressed, held.contains(where: { $0.module === module && $0.id == id }) {
            chord.pressed = held.filter { !($0.module === module && $0.id == id) }
            module.shortcut("globalshortcutup", keys: keys, id: id)
        }
        chord.readers.removeAll { $0.module == nil || ($0.module === module && $0.id == id) }
        guard chord.readers.isEmpty else { return }
        if let ref = chord.ref { UnregisterEventHotKey(ref) }
        chords[keys] = nil
        if !chords.keys.contains(where: { Self.modifier($0) != nil }) { removeMonitors() }
    }

    private func press(_ keys: String, _ chord: Chord) {
        // Carbon reports distinct presses. Either input path can miss a lift across sleep.
        lift(keys, chord)
        chord.readers.removeAll { $0.module == nil }
        guard let target = chord.readers.first(where: { $0.module?.isKey == true })?.module
                ?? chord.readers.first?.module else { return }
        let recipients = chord.readers.filter { $0.module === target }
        chord.pressed = recipients
        for reader in recipients { target.shortcut("globalshortcut", keys: keys, id: reader.id) }
    }

    private func lift(_ keys: String, _ chord: Chord) {
        let recipients = chord.pressed ?? []
        chord.pressed = nil
        for reader in recipients { reader.module?.shortcut("globalshortcutup", keys: keys, id: reader.id) }
    }

    // MARK: Carbon

    private func register(_ keys: String, number: UInt32) -> (EventHotKeyRef?, String?) {
        guard let (code, modifiers) = Self.carbon(keys) else { return (nil, "invalid") }
        // macOS 15: a hot key needs a modifier other than Shift or Option
        // (Option alone again from 15.2).
        if modifiers & UInt32(cmdKey | controlKey) == 0, modifiers != UInt32(optionKey) { return (nil, "reserved") }
        var ref: EventHotKeyRef?
        let status = RegisterEventHotKey(code, modifiers, EventHotKeyID(signature: Self.signature, id: number),
                                         GetApplicationEventTarget(), 0, &ref)
        switch status {
        case noErr: return (ref, nil)
        case OSStatus(eventHotKeyExistsErr): return (nil, "in-use")
        default: return (nil, "reserved")
        }
    }

    private func installHandler() -> Bool {
        if handler != nil { return true }
        let events = [kEventHotKeyPressed, kEventHotKeyReleased].map {
            EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32($0))
        }
        let callback: EventHandlerUPP = { _, event, _ in
            var key = EventHotKeyID()
            guard GetEventParameter(event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID),
                                    nil, MemoryLayout<EventHotKeyID>.size, nil, &key) == noErr,
                  key.signature == HotKeys.signature else { return OSStatus(eventNotHandledErr) }
            let number = key.id
            let isRelease = GetEventKind(event) == UInt32(kEventHotKeyReleased)
            DispatchQueue.main.async {
                let shared = HotKeys.shared
                guard let (keys, chord) = shared.chords.first(where: { $0.value.number == number }) else { return }
                if isRelease {
                    shared.lift(keys, chord)
                } else {
                    shared.press(keys, chord)
                }
            }
            return noErr
        }
        return events.withUnsafeBufferPointer {
            InstallEventHandler(GetApplicationEventTarget(), callback, $0.count, $0.baseAddress, nil, &handler) == noErr
        }
    }

    // MARK: Modifier shortcuts

    private static func modifier(_ keys: String) -> (first: UInt16, second: UInt16?, mask: UInt)? {
        switch keys {
        case "MetaRight":
            (UInt16(kVK_RightCommand), nil, UInt(NX_DEVICERCMDKEYMASK))
        case "AltRight":
            (UInt16(kVK_RightOption), nil, UInt(NX_DEVICERALTKEYMASK))
        case "Shift+Shift":
            (UInt16(kVK_Shift), UInt16(kVK_RightShift), UInt(NX_DEVICELSHIFTKEYMASK | NX_DEVICERSHIFTKEYMASK))
        case "Control+Control":
            (UInt16(kVK_Control), UInt16(kVK_RightControl), UInt(NX_DEVICELCTLKEYMASK | NX_DEVICERCTLKEYMASK))
        case "Alt+Alt":
            (UInt16(kVK_Option), UInt16(kVK_RightOption), UInt(NX_DEVICELALTKEYMASK | NX_DEVICERALTKEYMASK))
        case "Meta+Meta":
            (UInt16(kVK_Command), UInt16(kVK_RightCommand), UInt(NX_DEVICELCMDKEYMASK | NX_DEVICERCMDKEYMASK))
        default: nil
        }
    }

    /// `.flagsChanged` only, in this app and in others: which modifier keys
    /// are down, and nothing else.
    private func installMonitors() -> Bool {
        if !monitors.isEmpty { return true }
        let heard: (NSEvent) -> Void = { [weak self] event in self?.flags(event) }
        guard let global = NSEvent.addGlobalMonitorForEvents(matching: .flagsChanged, handler: heard) else { return false }
        guard let local = NSEvent.addLocalMonitorForEvents(matching: .flagsChanged, handler: { heard($0); return $0 }) else {
            NSEvent.removeMonitor(global)
            return false
        }
        monitors = [global, local]
        return true
    }

    private func removeMonitors() {
        monitors.forEach(NSEvent.removeMonitor)
        monitors = []
        lastFlagsEvent = nil
    }

    private func flags(_ event: NSEvent) {
        if let last = lastFlagsEvent, last.timestamp == event.timestamp, last.keyCode == event.keyCode,
           last.modifierFlags == event.modifierFlags { return }
        lastFlagsEvent = event
        let code = event.keyCode
        // Device flags describe both sides at this event, even if the monitor missed an earlier change.
        for (keys, chord) in chords {
            guard let modifier = Self.modifier(keys) else { continue }
            if event.modifierFlags.rawValue & modifier.mask != modifier.mask {
                lift(keys, chord)
            } else if code == modifier.first || code == modifier.second {
                press(keys, chord)
            }
        }
    }

    // MARK: Spelling

    /// `aria-keyshortcuts` to Carbon's virtual key and modifiers.
    private static func carbon(_ keys: String) -> (UInt32, UInt32)? {
        let parts = keys.split(separator: "+").map(String.init)
        guard let key = parts.last, let code = code(key) else { return nil }
        var modifiers: UInt32 = 0
        for name in parts.dropLast() {
            switch name {
            case "Meta": modifiers |= UInt32(cmdKey)
            case "Control": modifiers |= UInt32(controlKey)
            case "Alt": modifiers |= UInt32(optionKey)
            case "Shift": modifiers |= UInt32(shiftKey)
            default: return nil
            }
        }
        return (UInt32(code), modifiers)
    }

    private static let letters: [Character: Int] = [
        "A": kVK_ANSI_A, "B": kVK_ANSI_B, "C": kVK_ANSI_C, "D": kVK_ANSI_D, "E": kVK_ANSI_E, "F": kVK_ANSI_F,
        "G": kVK_ANSI_G, "H": kVK_ANSI_H, "I": kVK_ANSI_I, "J": kVK_ANSI_J, "K": kVK_ANSI_K, "L": kVK_ANSI_L,
        "M": kVK_ANSI_M, "N": kVK_ANSI_N, "O": kVK_ANSI_O, "P": kVK_ANSI_P, "Q": kVK_ANSI_Q, "R": kVK_ANSI_R,
        "S": kVK_ANSI_S, "T": kVK_ANSI_T, "U": kVK_ANSI_U, "V": kVK_ANSI_V, "W": kVK_ANSI_W, "X": kVK_ANSI_X,
        "Y": kVK_ANSI_Y, "Z": kVK_ANSI_Z, "0": kVK_ANSI_0, "1": kVK_ANSI_1, "2": kVK_ANSI_2, "3": kVK_ANSI_3,
        "4": kVK_ANSI_4, "5": kVK_ANSI_5, "6": kVK_ANSI_6, "7": kVK_ANSI_7, "8": kVK_ANSI_8, "9": kVK_ANSI_9,
        "-": kVK_ANSI_Minus, "=": kVK_ANSI_Equal, "[": kVK_ANSI_LeftBracket, "]": kVK_ANSI_RightBracket,
        ";": kVK_ANSI_Semicolon, "'": kVK_ANSI_Quote, ",": kVK_ANSI_Comma, ".": kVK_ANSI_Period,
        "/": kVK_ANSI_Slash, "\\": kVK_ANSI_Backslash, "`": kVK_ANSI_Grave,
    ]
    private static let named: [String: Int] = [
        "Space": kVK_Space, "Enter": kVK_Return, "Escape": kVK_Escape, "Tab": kVK_Tab, "Backspace": kVK_Delete,
        "Delete": kVK_ForwardDelete, "ArrowUp": kVK_UpArrow, "ArrowDown": kVK_DownArrow, "ArrowLeft": kVK_LeftArrow,
        "ArrowRight": kVK_RightArrow, "Home": kVK_Home, "End": kVK_End, "PageUp": kVK_PageUp, "PageDown": kVK_PageDown,
        "F1": kVK_F1, "F2": kVK_F2, "F3": kVK_F3, "F4": kVK_F4, "F5": kVK_F5, "F6": kVK_F6, "F7": kVK_F7, "F8": kVK_F8,
        "F9": kVK_F9, "F10": kVK_F10, "F11": kVK_F11, "F12": kVK_F12, "F13": kVK_F13, "F14": kVK_F14, "F15": kVK_F15,
        "F16": kVK_F16, "F17": kVK_F17, "F18": kVK_F18, "F19": kVK_F19, "F20": kVK_F20,
    ]
    private static func code(_ key: String) -> Int? {
        if let code = named[key] { return code }
        guard key.count == 1, let c = key.uppercased().first else { return nil }
        return letters[c]
    }
}
