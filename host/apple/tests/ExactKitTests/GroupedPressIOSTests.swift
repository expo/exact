#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactGroupedLists

final class GroupedPressIOSTests: GroupedListFixture {
    func testViewlessNativeRowKeepsTheTitlesColorOwnership() throws {
        var face = ButtonFace()
        face.title = "Profile"
        face.rows.title["written"] = .array([.string("text_color")])
        var model = model()
        model.sections[0].rows[0].titleLook.color = .array([.number(0), .number(255), .number(0), .number(255)])
        let seed = Presenter()
        seed.buttonFace = { _ in face }
        let p = presenter(nativeButtonRow: true, on: seed) { model }
        let row = try XCTUnwrap(p.views[10])
        XCTAssertTrue(row.container.subviews.compactMap { $0 as? NodeView }.isEmpty)
        let native = try cell(p, 10)
        let style: [String: Any] = ["press_color": [0, 0, 255, 255]]
        p.apply(wireBatch([["op": "style", "id": 10, "style": style]]))
        native.isHighlighted = true
        native.setNeedsUpdateConfiguration()
        waitForMainQueue()
        func ink() -> UIColor? {
            (native.contentConfiguration as? UIListContentConfiguration)?.textProperties.color.resolvedColor(with: native.traitCollection)
        }
        XCTAssertEqual(ink(), .green)
        face.rows.title["written"] = nil
        p.controls.facesChanged()
        p.apply(wireBatch([["op": "style", "id": 10, "style": style]]))
        native.setNeedsUpdateConfiguration()
        waitForMainQueue()
        XCTAssertEqual(ink(), .blue)
    }

    /// A row's press ends the editing under way, unless the row or its
    /// section retains the focus, a carried row's too: then the field stays
    /// first responder and commits only as it resigns.
    func testARowsPressLeavesTheEditingAloneUnderRetainFocus() throws {
        guard let scene = hostedWindowScene() else { throw missing("scene", "a field takes first responder only in the hosted app") }
        for (row, retaining) in [(10, nil), (10, 10), (21, nil), (21, 3)] as [(UInt32, Int?)] {
            let p = presenter { self.model(custom: true) }
            p.apply(wireBatch([
                ["op": "create", "id": 30, "kind": "input", "props": [:], "handlers": ["change", "blur"], "style": ["text_color": [0, 0, 0, 255]]],
                ["op": "frame", "id": 30, "x": 0.0, "y": 800.0, "w": 300.0, "h": 34.0],
                ["op": "roots", "ids": [1, 30]],
            ] + (retaining.map { [["op": "props", "id": $0, "set": ["retainFocus": "true"]]] } ?? [])))
            window.windowScene = scene
            window.makeKeyAndVisible()
            _ = try cell(p, row)
            XCTAssertEqual(host(p).logicalParent(of: try XCTUnwrap(p.views[row])) != nil, row == 21, "row 21 is carried into its cell")
            var events: [String] = []
            p.onPress = { events.append("press \($0)") }
            p.onChange = { events.append("change \($0) \($1)") }
            p.onBlur = { events.append("blur \($0)") }
            let field = try XCTUnwrap(p.views[30]?.field)
            XCTAssertTrue(field.becomeFirstResponder())
            field.text = "Hi"
            p.typed(30, "Hi", input: false)
            XCTAssertTrue(try list(p).select(row))
            let step = "row \(row), retainFocus on \(retaining.map(String.init) ?? "nothing")"
            XCTAssertEqual(field.isFirstResponder, retaining != nil, step)
            field.resignFirstResponder()
            let commit = ["change 30 Hi", "blur 30"], press = ["press \(row)"]
            XCTAssertEqual(events, retaining == nil ? commit + press : press + commit, step)
            window.isHidden = true
        }
    }

    func testHighlightedTranslationFollowsBoundsIncludingAnInitiallyEmptyCell() throws {
        window = UIWindow(frame: CGRect(x: 0, y: 0, width: 400, height: 800))
        window.makeKeyAndVisible()
        defer { window.isHidden = true }
        for initialWidth in [CGFloat(0), 100] {
            let cell = GroupedCell(frame: CGRect(x: 0, y: 0, width: initialWidth, height: 40))
            window.addSubview(cell)
            defer { cell.removeFromSuperview() }
            cell.activePaint(["press_translate_percent": .array([.number(50), .number(0)])],
                             content: .cell(), enabled: true, protected: (false, false, false))
            var configured = false
            let update = cell.configurationUpdateHandler
            cell.configurationUpdateHandler = { cell, state in
                update?(cell, state)
                configured = true
            }
            cell.forcedPressed = true
            waitFor("the initial highlighted configuration", late: 1, cap: 2) { configured }
            XCTAssertEqual((cell.layer.animation(forKey: "pressTranslation") as? CABasicAnimation)?.toValue as? NSValue,
                           initialWidth == 0 ? nil : NSValue(cgPoint: CGPoint(x: 50, y: 0)))
            cell.bounds.size.width = 200
            cell.layoutSubviews()
            waitForMainQueue()
            let animation = try XCTUnwrap(cell.layer.animation(forKey: "pressTranslation") as? CABasicAnimation)
            XCTAssertEqual((animation.toValue as? NSValue)?.cgPointValue, CGPoint(x: 100, y: 0))
            XCTAssertEqual(animation.duration, .greatestFiniteMagnitude, "native configuration adds no ease")
            cell.forcedPressed = false
            waitFor("the released configuration", late: 1, cap: 2) { cell.layer.animation(forKey: "pressTranslation") == nil }
            XCTAssertNil(cell.layer.animation(forKey: "pressTranslation"))
        }
    }

    func testActiveScaleKeepsTheAuthoredOriginFixed() throws {
        let p = presenter { self.model() }
        let cell = try XCTUnwrap(try cell(p, 10) as? GroupedCell)
        let content = try XCTUnwrap(cell.contentConfiguration as? UIListContentConfiguration)
        cell.activePaint(["press_scale": .number(0.5), "transform_origin": .array([.number(0), .number(0)])],
                         content: content, enabled: true, protected: (false, false, false))
        cell.isHighlighted = true
        cell.setNeedsUpdateConfiguration()
        waitFor("the active scale", late: 1, cap: 2) { abs((cell.layer.presentation()?.sublayerTransform.m11 ?? 1) - 0.5) < 0.001 }
        let transform = try XCTUnwrap(cell.layer.presentation()).sublayerTransform
        let anchor = CGPoint(x: cell.bounds.width * cell.layer.anchorPoint.x, y: cell.bounds.height * cell.layer.anchorPoint.y)
        XCTAssertEqual(anchor.x * (1 - transform.m11) + transform.m41, 0, accuracy: 0.01, "the top-left origin stays fixed")
        XCTAssertEqual(anchor.y * (1 - transform.m22) + transform.m42, 0, accuracy: 0.01)
        XCTAssertEqual(cell.layer.sublayerTransform.m11, 1, "tracking keeps its native geometry")
        cell.isHighlighted = false
    }

    func testPartialActivePropertiesKeepTheNativeHighlightedConfiguration() throws {
        let cases: [NodeStyle] = [
            ["press_scale": .number(0.94)], ["press_opacity_to": .number(0.4)],
            ["press_color": .array([.number(255), .number(0), .number(0), .number(255)])],
        ]
        for style in cases {
            let p = presenter { self.model() }
            let native = try XCTUnwrap(try cell(p, 10) as? GroupedCell)
            let content = try XCTUnwrap(native.contentConfiguration as? UIListContentConfiguration)
            let ground = try XCTUnwrap(native.backgroundConfiguration)
            var highlighted: Bool?
            native.configurationUpdateHandler = { cell, state in
                highlighted = state.isHighlighted
                cell.backgroundConfiguration = ground.updated(for: state)
            }
            native.activePaint(style, content: content, enabled: true, protected: (false, false, false))
            native.isHighlighted = true
            native.setNeedsUpdateConfiguration()
            waitForMainQueue()
            XCTAssertEqual(highlighted, true, "a property override keeps UIKit's native state")
            let expected = ground.updated(for: native.configurationState)
            XCTAssertEqual(native.backgroundConfiguration?.backgroundColor?.resolvedColor(with: native.traitCollection),
                           expected.backgroundColor?.resolvedColor(with: native.traitCollection), "an unwritten background stays native")
            if style["press_color"] == nil {
                let expected = content.updated(for: native.configurationState)
                XCTAssertEqual((native.contentConfiguration as? UIListContentConfiguration)?.textProperties.color.resolvedColor(with: native.traitCollection),
                               expected.textProperties.color.resolvedColor(with: native.traitCollection))
            }
            native.isHighlighted = false
            window.isHidden = true
        }
    }

    func testActiveInkUsesTheLeadingSymbolsColorOwnership() throws {
        for ownsLeading in [false, true] {
            let p = presenter { self.model() }
            let red: [String: Any] = ["text_color": [255, 0, 0, 255], "written": ["text_color"]]
            p.apply(wireBatch([
                ["op": "create", "id": 30, "kind": "image", "props": ["imageSource": "symbol:sf/xmark"], "style": red.merging(["display": "none"]) { _, new in new }],
                ["op": "create", "id": 31, "kind": "image", "props": ["imageSource": "symbol:sf/person.circle", "symbolColorWritten": ownsLeading ? "1" : "0"], "style": ownsLeading ? red : [:]],
                ["op": "create", "id": 33, "kind": "text", "props": ["text": "Profile"], "style": [:]],
                ["op": "create", "id": 32, "kind": "image", "props": ["imageSource": "symbol:forward-chevron"], "style": ownsLeading ? [:] : red],
                ["op": "children", "id": 10, "ids": [30, 31, 33, 32]],
                ["op": "style", "id": 10, "style": ["press_color": [0, 0, 255, 255]]],
            ]))
            let native = try cell(p, 10)
            native.isHighlighted = true
            native.setNeedsUpdateConfiguration()
            waitForMainQueue()
            let content = try XCTUnwrap(native.contentConfiguration as? UIListContentConfiguration)
            XCTAssertEqual(content.image?.renderingMode, ownsLeading ? .alwaysOriginal : .alwaysTemplate)
            XCTAssertEqual(content.textProperties.color.resolvedColor(with: native.traitCollection), .blue)
            if ownsLeading {
                XCTAssertEqual((native as? GroupedCell)?.imageInk?.resolvedColor(with: native.traitCollection), .red)
            } else {
                XCTAssertEqual(content.imageProperties.tintColor?.resolvedColor(with: native.traitCollection), .blue)
            }
            XCTAssertFalse(native.accessories.isEmpty, "the native disclosure accessory stays")
            native.isHighlighted = false
            window.isHidden = true
        }
    }

    func testAStandardCellShowsAuthoredActivePaintWithoutReplacingItsContent() throws {
        let p = presenter { self.model() }
        let native = try cell(p, 10)
        native.isHighlighted = true
        native.setNeedsUpdateConfiguration()
        waitForMainQueue()
        let style: [String: Any] = ["press_background_color": [255, 0, 0, 255], "press_color": [0, 0, 255, 255], "press_opacity_to": 0.4, "press_scale": 0.95]
        p.apply(wireBatch([["op": "style", "id": 10, "style": style]]))
        waitForMainQueue()
        native.setNeedsUpdateConfiguration()
        waitForMainQueue()
        XCTAssertEqual(native.backgroundConfiguration?.backgroundColor?.resolvedColor(with: native.traitCollection), .red)
        XCTAssertEqual((native.contentConfiguration as? UIListContentConfiguration)?.text, "Profile")
        XCTAssertEqual((native.contentConfiguration as? UIListContentConfiguration)?.textProperties.color.resolvedColor(with: native.traitCollection), .blue)
        waitFor("the cell's active presentation", late: 1, cap: 2) {
            guard let shown = native.layer.presentation() else { return false }
            return abs(shown.opacity - 0.4) < 0.01 && abs(shown.sublayerTransform.m11 - 0.95) < 0.01
        }
        XCTAssertEqual(native.alpha, 1, "feedback leaves the model opacity and hit testing alone")
        p.apply(wireBatch([["op": "style", "id": 10, "style": [:]]]))
        waitForMainQueue()
        native.setNeedsUpdateConfiguration()
        waitForMainQueue()
        XCTAssertNotEqual(native.backgroundConfiguration?.backgroundColor?.resolvedColor(with: native.traitCollection), .red)
        XCTAssertEqual(native.alpha, 1, accuracy: 0.001)
        XCTAssertEqual(native.transform.a, 1, accuracy: 0.001)
        native.isHighlighted = false
    }

    func testAHeldStateUsesTheStandardCellsHighlight() throws {
        let p = presenter { self.model() }
        let row = try XCTUnwrap(p.views[10]), native = try cell(p, 10)
        XCTAssertEqual(row.hold("active", dropped: nil).held, "active")
        defer { _ = row.hold(nil, dropped: nil) }
        p.apply(wireBatch([["op": "style", "id": 10, "style": ["press_background_color": [255, 0, 0, 255]]]]))
        waitFor("the held native cell", late: 1, cap: 2) {
            native.backgroundConfiguration?.backgroundColor?.resolvedColor(with: native.traitCollection) == .red
        }
    }

    func testInkWrittenBelowTheRowStaysOwnedUntilItStartsInheriting() throws {
        var m = model()
        m.sections[0].rows[0].titleLook.color = .array([.number(0), .number(255), .number(0), .number(255)])
        let p = presenter { m }
        let native = try cell(p, 10)
        let font = (native.contentConfiguration as? UIListContentConfiguration)?.textProperties.font
        p.apply(wireBatch([
            ["op": "create", "id": 101, "kind": "text", "props": ["text": "Profile"], "style": ["text_color": [0, 255, 0, 255], "written": ["text_color"]]],
            ["op": "children", "id": 10, "ids": [101]],
            ["op": "style", "id": 10, "style": ["text_color": [0, 255, 0, 255], "written": ["text_color"], "press_background_color": [255, 0, 0, 255], "press_color": [0, 0, 255, 255]]],
        ]))
        native.isHighlighted = true
        native.setNeedsUpdateConfiguration()
        waitFor("the row's authored active ground", late: 1, cap: 2) {
            native.backgroundConfiguration?.backgroundColor?.resolvedColor(with: native.traitCollection) == .red
        }
        XCTAssertEqual((native.contentConfiguration as? UIListContentConfiguration)?.textProperties.color.resolvedColor(with: native.traitCollection), .green)
        p.apply(wireBatch([["op": "style", "id": 101, "style": ["text_color": [0, 255, 0, 255]]]]))
        waitFor("the title inheriting the active ink", late: 1, cap: 2) {
            (native.contentConfiguration as? UIListContentConfiguration)?.textProperties.color.resolvedColor(with: native.traitCollection) == .blue
        }
        XCTAssertEqual((native.contentConfiguration as? UIListContentConfiguration)?.textProperties.font, font)
        native.isHighlighted = false
    }
}
#endif
