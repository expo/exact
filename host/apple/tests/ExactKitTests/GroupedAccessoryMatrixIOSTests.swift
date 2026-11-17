#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists
import ExactMarkdown

/// The rule of `GroupedAccessoriesIOS.swift`, row state by row state. Each
/// state's Markdown row stands beside its plain twin, which UIKit's own cell
/// draws, in `scripts/fixtures/grouped-list-reveal.contract`: the two say the
/// same, and the Markdown row's text takes the room the plain cell's does.
final class GroupedAccessoryMatrixIOSTests: XCTestCase {
    /// What `accessory-update` flips for a row: its control or the row left
    /// out of the accessibility tree, by itself or by its section, its
    /// control unavailable, its detail button disabled, or its switch hidden
    /// by its author (`visibility`).
    private enum Flip { case controlHidden, rowHidden, sectionHidden, disabled, ariaDisabled, detailDisabled, unseen }
    /// A row state: its name in the fixture, the accessory UIKit draws, what
    /// flips, and its name where the row's words are not it.
    private typealias State = (shape: String, accessory: String, flip: Flip?, label: String?)
    private static let words = "A long network name that fills the available line completely"
    private static let states: [State] = [
        ("disclosure", "disclosure", nil, nil),
        ("checkmark", "checkmark", nil, nil),
        ("value", "disclosure", nil, "\(words), On"),
        ("radio", "radio", nil, nil),
        ("radio-labelled", "radio", nil, "Home network"),
        ("radio-disabled", "radio", .disabled, nil),
        ("radio-aria-disabled", "radio", .ariaDisabled, nil),
        ("radio-hidden", "radio", .controlHidden, nil),
        ("toggle", "toggle", nil, nil),
        ("toggle-disabled", "toggle", .disabled, nil),
        ("toggle-aria-disabled", "toggle", .ariaDisabled, nil),
        ("toggle-hidden", "toggle", .controlHidden, nil),
        // Its switch is labelled by a subtree that is not shown, which still names it.
        ("toggle-named", "toggle", nil, "Notifications"),
        ("detail", "detail", nil, nil),
        ("detail-disabled", "detail", .detailDisabled, nil),
        // Its title has a label of its own, said before the words it shows.
        ("title-labelled", "checkmark", nil, "Elapsed time, \(words)"),
        // Its title holds a run that is not shown, which changes nothing.
        ("hidden-run", "disclosure", nil, nil),
        ("row-hidden", "toggle", .rowHidden, nil),
        ("toggle-unseen", "toggle", .unseen, nil),
        ("section-hidden-toggle", "toggle", .sectionHidden, nil),
        ("section-hidden-radio", "radio", .sectionHidden, nil),
    ]

    private var window: UIWindow!
    private var session: ExactSession!
    private var p: Presenter { session.presenter }
    private var host: GroupedListHost { p.groupedLists as! GroupedListHost }

    private func node(_ name: String) throws -> NodeView {
        try XCTUnwrap(p.views.values.first { $0.props["testId"] == name }, name)
    }
    private func list(_ row: NodeView) throws -> GroupedListView {
        try XCTUnwrap(host.lists.values.first { $0.rows[row.id] != nil })
    }
    /// The row scrolled into view as a finger scrolls it: no batch follows,
    /// so what a cell needs once it shows is the host's to do.
    private func reveal(_ row: NodeView) throws -> GroupedCell {
        XCTAssertNil(Agent(session: session).reveal(["id": Int(row.id)])["error"])
        window.layoutIfNeeded()
        waitForMainQueue()
        return try XCTUnwrap(try list(row).cell(row.id) as? GroupedCell, row.props["testId"] ?? "")
    }
    private func settle() {
        window.layoutIfNeeded()
        XCTAssertNil(Agent(session: session).clock(["settle": true])["error"])
    }
    private func elements(_ root: UIView) -> [[String: Any]] {
        p.axElements(roots: [root])["elements"] as? [[String: Any]] ?? []
    }
    /// The switches under `view` that show.
    private func switches(_ view: UIView) -> [UISwitch] {
        view.isHidden ? [] : ((view as? UISwitch).map { [$0] } ?? []) + view.subviews.flatMap(switches)
    }
    /// The string a Markdown text draws, less the soft hyphens its lines may
    /// break at, and whether `word` in it is bold.
    private func drawn(_ title: NodeView, bold word: String) -> (String, Bool) {
        let text = session.text.attributed(title.paragraphSpec())
        let string = text.string.replacingOccurrences(of: "\u{AD}", with: "")
        let at = (text.string as NSString).range(of: word)
        guard at.location != NSNotFound, let font = text.attribute(.font, at: at.location, effectiveRange: nil) else { return (string, false) }
        return (string, CTFontGetSymbolicTraits(font as! CTFont).contains(.traitBold))
    }

    func testEveryRowStateFollowsOneRuleInMarkdownAndPlainRows() throws {
        guard Presenter.axRuntimeLoaded else { throw missing("accessibility", "UIKit's switch traits and element frames need its accessibility runtime") }
        ExactMarkdown.install()
        ExactGroupedLists.install()
        session = ExactApp.shared.makeSession(label: "accessory-matrix")
        defer { session.destroy(); window?.isHidden = true }
        let view = ExactView(session: session), root = UIViewController()
        // The test host connects its scene as it launches, which the first test of a run can be ahead of.
        waitFor("the host's window scene", cap: 10, onFailure: { _ in }) { UIApplication.shared.connectedScenes.first is UIWindowScene }
        guard let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene else {
            throw missing("scene", "the list needs a hosted window")
        }
        window = UIWindow(windowScene: scene)
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = root
        window.makeKeyAndVisible()
        root.view.addSubview(view)
        view.frame = root.view.bounds
        let path = try XCTUnwrap(ProcessInfo.processInfo.environment["EXACT_GROUPED_REVEAL_PLAN"])
        XCTAssertNil(session.boot(plan: try Data(contentsOf: URL(fileURLWithPath: path)), size: view.bounds.size).error)
        settle()
        let agent = Agent(session: session)

        // Which Markdown rows have a cell before anything scrolls: the rest are first shown by a scroll.
        var first: [String: Bool] = [:]
        for state in Self.states {
            let row = try node("projection-\(state.shape)-true")
            first[state.shape] = try list(row).cell(row.id) != nil
        }
        XCTAssertEqual(Set(first.values), [true, false], "the fixture has rows of both kinds: \(first)")

        /// The Markdown row's text has the room UIKit gives its plain twin's content.
        func room(_ state: State, _ name: String) throws {
            let row = try node("projection-\(state.shape)-true"), title = try node("title-\(state.shape)-true")
            let plain = try reveal(try node("projection-\(state.shape)-false"))
            plain.layoutIfNeeded()
            let reference = plain.contentView.bounds.inset(by: plain.contentView.layoutMargins)
            let cell = try reveal(row)
            let last = state.shape == "value" ? try node("value-true") : title
            func frames() -> (CGRect, CGRect) { (title.convert(title.bounds, to: cell.contentView), last.convert(last.bounds, to: cell.contentView)) }
            waitFor("the room beside the accessory: \(name)", cap: 5, facts: { "text \(frames()), UIKit's content \(reference)" }) {
                abs(frames().0.minX - reference.minX) <= 1 && abs(frames().1.maxX - reference.maxX) <= 1
            }
        }
        // First with no batch at all: a row a scroll shows has its room from its cell alone.
        // (A row that is `aria-hidden`, or whose switch its author hid, keeps its own views, its switch among
        // them: the kernel's rule.)
        for state in Self.states where state.flip != .rowHidden && state.flip != .unseen {
            try room(state, "\(state.shape), first shown \(first[state.shape] == true ? "at once" : "by a scroll")")
        }

        /// The room a toggle row leaves where UIKit has no switch, as its control was found on and off.
        var switchless: Set<String> = []
        /// One row in one state: `flipped` is whether what its state flips is in force.
        func check(_ state: State, markdown: Bool, flipped: Bool, activating: Bool = true) throws {
            let name = "\(state.shape), Markdown \(markdown), flipped \(flipped)"
            let row = try node("projection-\(state.shape)-\(markdown)"), title = try node("title-\(state.shape)-\(markdown)")
            let control = try node("control-\(state.shape)-\(markdown)"), list = try list(row)
            let on = flipped && state.flip != nil
            let excluded = on && (state.flip == .rowHidden || state.flip == .sectionHidden)
            let heard = !(on && state.flip == .controlHidden)
            let unavailable = on && (state.flip == .disabled || state.flip == .ariaDisabled)
            // A detail button takes a press only while the row's own button would.
            let presses = !(on && state.flip == .detailDisabled)
            // A row that is itself `aria-hidden`, or whose switch its author hid, keeps its views, and UIKit
            // draws it no accessory.
            let projected = !(on && (state.flip == .rowHidden || state.flip == .unseen))
            var cell = try reveal(row)

            if markdown {
                // The text is its own view, drawing the Markdown.
                let (string, bold) = drawn(title, bold: "long")
                XCTAssertEqual(string, Self.words, name)
                XCTAssertTrue(bold, "the emphasis is drawn: \(name)")
                XCTAssertTrue(title.isDescendant(of: cell.contentView), name)
                XCTAssertNil(cell.contentConfiguration, name)
            }
            if markdown, projected {
                // The accessory is UIKit's, in place of the view its author wrote.
                XCTAssertTrue(control.isHidden, name)
                XCTAssertTrue(host.draws(control.id), name)
                let visible = list.accessory(row.id) ?? cell
                // Equal but for the rounding of a conversion between views at a fractional offset.
                let projected = try XCTUnwrap(list.projectedRect(for: control), name), frame = visible.convert(visible.bounds, to: list.collection)
                XCTAssertTrue(projected.insetBy(dx: -0.01, dy: -0.01).contains(frame) && frame.insetBy(dx: -0.01, dy: -0.01).contains(projected), "\(projected) \(frame): \(name)")
                if case .view(let aim, _)? = host.shown(control) { XCTAssertTrue(aim === visible, name) }
                else { XCTFail("no aim: \(name)") }
            }

            // Left out of the tree with its row or its section; else one element, the cell.
            XCTAssertEqual(cell.accessibilityElementsHidden, excluded, name)
            XCTAssertEqual(cell.isAccessibilityElement, projected, name)
            if on, state.flip == .unseen {
                // The switch keeps its room and shows nothing, and no switch of UIKit's stands in for it:
                // the row is its button, named by its words.
                XCTAssertNil(cell.contentConfiguration, name)
                XCTAssertTrue(title.isDescendant(of: cell.contentView) && control.isDescendant(of: cell.contentView), name)
                XCTAssertTrue(cell.accessories.isEmpty, name)
                XCTAssertFalse(host.draws(control.id), name)
                XCTAssertFalse(control.isHidden, name)
                XCTAssertGreaterThan(control.frame.width, 0, name)
                XCTAssertEqual(switches(cell), [], name)
                let found = elements(cell)
                XCTAssertEqual(found.map { $0["name"] as? String }, [Self.words], name)
                XCTAssertEqual(found.map { $0["role"] as? String }, ["button"], name)
                return
            }
            let found = elements(cell)
            XCTAssertEqual(found.count, excluded ? 0 : 1, name)
            if excluded { return }
            XCTAssertEqual((found.first?["native"] as? [String: Any])?["class"] as? String, "GroupedCell", name)

            // Named by the row, else its control, else its words; never by what is not shown.
            let label = heard ? state.label ?? Self.words : Self.words
            XCTAssertEqual(cell.accessibilityLabel, label, name)
            XCTAssertEqual(found.first?["name"] as? String, label, name)

            // The control's value and traits, unless its author hid it.
            XCTAssertTrue(cell.accessibilityTraits.contains(.button), name)
            XCTAssertEqual(cell.accessibilityTraits.contains(.notEnabled), unavailable, name)
            let toggle = list.accessory(row.id) as? UISwitch
            let checked = control.props["checked"] == "true"
            switch state.accessory {
            case "toggle":
                // UIKit's switch is as wide on as off. Where it has none (tvOS), the row leaves a checkmark's room or none.
                let model = try XCTUnwrap(list.rows[row.id], name)
                XCTAssertEqual(list.slot(model), "toggle", name)
                XCTAssertEqual(list.slot(model, switchless: true), checked ? "checkmark" : "none", name)
                switchless.insert(list.slot(model, switchless: true))
                XCTAssertEqual(toggle?.accessibilityElementsHidden, !heard, name)
                XCTAssertEqual(cell.accessibilityValue, heard ? toggle?.accessibilityValue : nil, name)
                XCTAssertEqual(cell.accessibilityValue != nil, heard, name)
                XCTAssertEqual(cell.accessibilityTraits.contains(.toggleButton), heard, name)
                XCTAssertEqual(found.first?["role"] as? String, heard ? "switch" : "button", name)
                XCTAssertEqual(cell.inspectionViewId, heard ? control.id : nil, name)
            case "radio":
                XCTAssertEqual(cell.accessibilityTraits.contains(.selected), checked && heard, name)
                XCTAssertEqual(cell.accessories.map(\.accessoryType), checked ? [.checkmark] : [], name)
            case "checkmark":
                XCTAssertTrue(cell.accessibilityTraits.contains(.selected), name)
            case "detail":
                XCTAssertEqual((list.accessory(row.id) as? UIButton)?.isEnabled, presses, name)
                XCTAssertEqual(list.detailTarget(try XCTUnwrap(list.rows[row.id])), presses ? control.id : nil, name)
            default: break
            }

            // Activating the cell acts on a control in the tree; else VoiceOver touches the row.
            guard activating else { return }
            switch (state.accessory, heard) {
            case ("toggle", true):
                let was = try XCTUnwrap(toggle).isOn, enabled = !(on && state.flip == .disabled)
                XCTAssertTrue(cell.accessibilityActivate(), name)
                settle()
                XCTAssertEqual(try XCTUnwrap(list.accessory(row.id) as? UISwitch).isOn, was != enabled, name)
            case ("radio", true):
                let enabled = !(on && state.flip == .disabled)
                XCTAssertEqual(cell.accessibilityActivate(), enabled, name)
                settle()
                cell = try reveal(row)
                XCTAssertEqual(cell.accessibilityTraits.contains(.selected), enabled, name)
                XCTAssertEqual(cell.accessories.map(\.accessoryType), enabled ? [.checkmark] : [], name)
                XCTAssertEqual(agent.tap(["id": Int(control.id)])["error"] == nil, enabled, name)
                settle()
                // The checkmark it now draws takes room from the text.
                if markdown { try room(state, "checked: \(name)") }
            default:
                XCTAssertFalse(cell.accessibilityActivate(), name)
                XCTAssertTrue(cell.accessibilityFrame.contains(cell.accessibilityActivationPoint), name)
                if let toggle { XCTAssertFalse(toggle.accessibilityFrame.contains(cell.accessibilityActivationPoint), name) }
                if heard {
                    XCTAssertEqual(agent.tap(["id": Int(control.id)])["error"] == nil, presses, name)
                    settle()
                }
            }
        }

        // Every state as the fixture starts, with what flips in force.
        for state in Self.states {
            for markdown in [false, true] { try check(state, markdown: markdown, flipped: true) }
        }
        // What flips changes on the row, its section or its control alone, both ways, and the cell follows.
        for flipped in [false, true] {
            let update = try node("accessory-update")
            _ = try reveal(update)
            XCTAssertNil(agent.tap(["id": Int(update.id)])["error"])
            settle()
            for state in Self.states where state.flip != nil {
                // Flipped back, a row is as the first pass found it: what it says is checked again.
                for markdown in [false, true] { try check(state, markdown: markdown, flipped: flipped, activating: !flipped) }
                // A row a batch gave its accessory has the room beside it too.
                if !flipped, state.flip == .rowHidden || state.flip == .unseen { try room(state, "\(state.shape), shown again") }
            }
        }

        XCTAssertEqual(switchless, ["checkmark", "none"], "the toggles were checked on and off")

        // A section UIKit draws is hidden by the host, not by its author: what holds the list still reads
        // the words of its standard rows (a live region, a name from content).
        let standard = try node("projection-none-false"), owner = try list(standard).owner
        let section = try XCTUnwrap(sequence(first: standard as UIView, next: \.superview).first { $0.superview === owner.container } as? NodeView)
        XCTAssertTrue(section.isHidden)
        XCTAssertTrue(owner.accessibleText.contains("A bold word in a list row"))

        // A text with a link or a press of its own, or an image with a press of its own where the accessory
        // would be, keeps each part of its row an element.
        for shape in ["link", "press", "image"] {
            for markdown in [false, true] {
                let name = "\(shape), Markdown \(markdown)"
                let row = try node("projection-\(shape)-\(markdown)"), control = try node("control-\(shape)-\(markdown)")
                let cell = try reveal(row)
                XCTAssertFalse(cell.isAccessibilityElement, name)
                XCTAssertFalse(control.isHidden, name)
                XCTAssertFalse(host.draws(control.id), name)
                XCTAssertTrue(cell.accessories.isEmpty, name)
                let found = elements(cell)
                func element(_ testId: String) -> [String: Any]? { found.first { $0["testId"] as? String == testId } }
                XCTAssertEqual(element("title-\(shape)-\(markdown)")?["name"] as? String, "Home", name)
                XCTAssertEqual(element("control-\(shape)-\(markdown)")?["role"] as? String, shape == "image" ? "image" : "switch", name)
                switch shape {
                case "link": XCTAssertEqual(element("help-\(markdown)")?["role"] as? String, "link", name)
                case "press": XCTAssertTrue(try node("title-press-\(markdown)").accessibilityActivate(), "its press is its own: \(name)")
                default:
                    // The image is drawn as written, and a tap on it runs its press, once for each twin.
                    XCTAssertNotNil(control.symbolView?.image, name)
                    XCTAssertNil(agent.tap(["id": Int(control.id)])["error"], name)
                    settle()
                    XCTAssertEqual(try node("control-image-\(markdown)").props["accessibilityLabel"], "More, pressed \(markdown ? 2 : 1)", name)
                }
            }
        }

        // A heading's row, which is no button, has a cell that is a heading: by itself, or with the switch it speaks for.
        for (shape, role) in [("heading", "heading"), ("heading-toggle", "switch")] {
            for markdown in [false, true] {
                let name = "\(shape), Markdown \(markdown)"
                let cell = try reveal(try node("projection-\(shape)-\(markdown)"))
                XCTAssertTrue(cell.isAccessibilityElement, name)
                XCTAssertEqual(cell.accessibilityLabel, "Home", name)
                XCTAssertTrue(cell.accessibilityTraits.contains(.header), name)
                // UIKit's switch is a button; the row is none.
                XCTAssertEqual(cell.accessibilityTraits.contains(.button), shape == "heading-toggle", name)
                let found = elements(cell)
                XCTAssertEqual(found.count, 1, name)
                XCTAssertEqual(found.first?["role"] as? String, role, name)
            }
        }

        // The row of issue 575, its text alone: no accessory, and the text draws its Markdown.
        let row = try node("projection-none-true"), title = try node("title-none-true")
        let cell = try reveal(row)
        let (string, bold) = drawn(title, bold: "bold")
        XCTAssertEqual(string, "A bold word in a list row")
        XCTAssertTrue(bold)
        XCTAssertTrue(title.isDescendant(of: cell.contentView))
        XCTAssertFalse(cell.isAccessibilityElement, "the text is the element")
        XCTAssertEqual(elements(cell).map { $0["name"] as? String }, ["A bold word in a list row"])

        // A cell UIKit keeps hidden once it scrolls away may hold an older row's accessories. It measures
        // nothing: its row takes the room its accessory last measured, so telling the rows' room settles.
        let kept = try node("projection-disclosure-true"), keptCell = try reveal(kept)
        let keptRow = try XCTUnwrap(try list(kept).rows[kept.id]), measured = try XCTUnwrap(try list(kept).room(for: keptRow))
        let accessories = keptCell.accessories
        keptCell.accessories = []
        keptCell.isHidden = true
        XCTAssertEqual(try list(kept).room(for: keptRow), measured)
        keptCell.isHidden = false
        keptCell.accessories = accessories

        // A radio unchecked while its cell is off screen, in a list that has measured no unchecked radio's
        // room: its row gives up the room its checkmark took, and has UIKit's when it shows again.
        let radio = try XCTUnwrap(Self.states.first { $0.shape == "radio" }), other = try node("projection-radio-labelled-true")
        let picked = try node("projection-radio-true"), radios = try list(picked)
        _ = try reveal(picked)
        XCTAssertTrue(radios.select(picked.id))
        settle()
        try room(radio, "checked on screen")
        guard case .edges(let taken)? = host.covered[picked.id] else { return XCTFail("the checked radio's row was told no room") }
        XCTAssertGreaterThan(taken.right, 0, "the checkmark takes room")
        XCTAssertEqual(radios.slot(try XCTUnwrap(radios.rows[picked.id])), "checkmark")
        _ = try reveal(try node("add-footer"))
        XCTAssertTrue((radios.cell(picked.id)?.isHidden ?? true) && (radios.cell(other.id)?.isHidden ?? true), "both radios are off screen")
        radios.rooms["none"] = nil
        XCTAssertTrue(radios.select(other.id))
        settle()
        waitForMainQueue()
        XCTAssertEqual(radios.slot(try XCTUnwrap(radios.rows[picked.id])), "none")
        XCTAssertNil(radios.rooms["none"], "no unchecked radio showed")
        XCTAssertNil(host.covered[picked.id], "the checkmark's room is given up")
        try room(radio, "unchecked off screen")
    }
}
#endif
