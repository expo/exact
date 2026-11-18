// The fixture's screen hand-built in AppKit, to measure against (`vo-trial.swift` drives both):
// `swiftc -O appkit-reference.swift -o appkit-reference && ./appkit-reference`, then
// `vo-trial appkit-reference "<item>"` (alternating it with the fixture, see `vo-trial.swift`). A window holds a long draft in an NSTextView ("Compose
// message") and a row "draft" whose right click pops up an NSMenu with
// `NSMenu.popUpContextMenu(_:with:for:)`, as Exact's MenuHost does. Each item does its work in
// its action, as AppKit sends it, or, "(after popUp)", once `popUpContextMenu` has returned, as
// Exact presses a pick:
//   Open Box, Field Focused     adds a group "Box" to the window and makes its field first responder
//                               (AppKit draws no modal in a window: the group stands for the
//                               fixture's `aria-modal` box, without its hiding of the page)
//   Delete Draft…               begins an NSAlert sheet
//   Rename…                     begins a sheet holding a field (its first responder) and Done
// Its standard error logs, by epoch milliseconds mod 1e6 (the clock `vo-trial` prints as
// `clock:`), the menu's notifications and its delegate's `menuDidClose`, each action and
// popUp's return: set beside the trial's AXMenuClosed, they say when AppKit acts.
// REFERENCE_X and REFERENCE_Y place the window's top left (screen points from the top left).
import AppKit

let draftText = "Dear team, the quarterly report is attached. It covers revenue, hiring, the two launches and the support backlog, with a page on what we would change next quarter and why. Please read it before Thursday and send me your questions, however small, so that the meeting can be about decisions rather than a walk through the numbers."

func log(_ s: String) {
    FileHandle.standardError.write("\(Int(Date().timeIntervalSince1970 * 1000) % 1_000_000) \(s)\n".data(using: .utf8)!)
}

final class Box: NSView {
    let field = NSTextField(string: "")
    init() {
        super.init(frame: NSRect(x: 40, y: 120, width: 520, height: 110))
        wantsLayer = true
        layer?.backgroundColor = NSColor.windowBackgroundColor.cgColor
        layer?.borderColor = NSColor.separatorColor.cgColor
        layer?.borderWidth = 1
        layer?.cornerRadius = 12
        field.frame = NSRect(x: 16, y: 60, width: 488, height: 24)
        field.setAccessibilityLabel("Box field")
        let close = NSButton(title: "Close", target: nil, action: #selector(removeFromSuperview))
        close.target = self
        close.frame = NSRect(x: 16, y: 16, width: 100, height: 30)
        addSubview(field); addSubview(close)
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
        setAccessibilityLabel("Box")
    }
    required init?(coder: NSCoder) { fatalError() }
}

final class Row: NSView {
    weak var controller: Controller?
    override func rightMouseDown(with event: NSEvent) {
        guard let controller else { return }
        controller.later = nil
        NSMenu.popUpContextMenu(controller.menu, with: event, for: self)
        log("popUp returned")
        controller.later?()
        controller.later = nil
    }
}

final class Controller: NSObject {
    let window: NSWindow
    let menu = NSMenu()
    var later: (() -> Void)?
    override init() {
        let env = ProcessInfo.processInfo.environment
        let x = Double(env["REFERENCE_X"] ?? "") ?? 60, y = Double(env["REFERENCE_Y"] ?? "") ?? 90
        window = NSWindow(contentRect: NSRect(x: x, y: NSScreen.main!.frame.height - y - 600, width: 600, height: 600),
                          styleMask: [.titled, .closable, .resizable, .miniaturizable], backing: .buffered, defer: false)
        window.title = "Compose"
        let scroll = NSScrollView(frame: NSRect(x: 16, y: 400, width: 568, height: 160))
        let text = NSTextView(frame: scroll.bounds)
        text.string = draftText
        text.isRichText = false
        text.autoresizingMask = [.width]
        text.setAccessibilityLabel("Compose message")
        scroll.documentView = text
        scroll.hasVerticalScroller = true
        super.init()
        let row = Row(frame: NSRect(x: 16, y: 340, width: 568, height: 40))
        row.controller = self
        let label = NSTextField(labelWithString: "draft")
        label.frame = NSRect(x: 12, y: 10, width: 200, height: 20)
        row.addSubview(label)
        window.contentView!.addSubview(scroll)
        window.contentView!.addSubview(row)
        for (title, work) in [("Open Box, Field Focused", showBox), ("Delete Draft…", alert), ("Rename…", rename)] as [(String, () -> Void)] {
            for deferred in [false, true] {
                let item = NSMenuItem(title: deferred ? "\(title) (after popUp)" : title, action: #selector(pick(_:)), keyEquivalent: "")
                item.target = self
                item.representedObject = { [weak self] in
                    log("action \(item.title)")
                    if deferred { self?.later = work } else { work() }
                }
                menu.addItem(item)
            }
        }
        menu.delegate = self
        for name in [NSMenu.didEndTrackingNotification, NSMenu.willSendActionNotification] {
            NotificationCenter.default.addObserver(forName: name, object: menu, queue: nil) { log($0.name.rawValue) }
        }
        window.makeKeyAndOrderFront(nil)
        window.makeFirstResponder(text)
    }
    @objc func pick(_ sender: NSMenuItem) { (sender.representedObject as? () -> Void)?() }
    func showBox() {
        let box = Box()
        window.contentView!.addSubview(box)
        window.makeFirstResponder(box.field)
        log("box shown, its field first responder")
    }
    func alert() {
        let alert = NSAlert()
        alert.messageText = "Delete Draft?"
        alert.informativeText = "This can't be undone."
        alert.addButton(withTitle: "Delete").hasDestructiveAction = true
        alert.addButton(withTitle: "Cancel").keyEquivalent = "\u{1b}"
        alert.beginSheetModal(for: window) { _ in }
        log("alert sheet begun")
    }
    func rename() {
        let sheet = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 320, height: 110), styleMask: [.titled], backing: .buffered, defer: false)
        sheet.title = "Rename"
        let field = NSTextField(string: "")
        field.frame = NSRect(x: 20, y: 64, width: 280, height: 24)
        field.setAccessibilityLabel("Title")
        let done = NSButton(title: "Done", target: nil, action: nil)
        done.frame = NSRect(x: 200, y: 16, width: 100, height: 30)
        sheet.contentView!.addSubview(field); sheet.contentView!.addSubview(done)
        sheet.initialFirstResponder = field
        window.beginSheet(sheet) { _ in }
        log("sheet begun")
    }
}

extension Controller: NSMenuDelegate {
    func menuDidClose(_ menu: NSMenu) { log("menuDidClose") }
}

let app = NSApplication.shared
app.setActivationPolicy(.regular)
let controller = Controller()
app.activate(ignoringOtherApps: true)
app.run()
