#if os(macOS)
import AppKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

final class RouteSheetPushMacTests: XCTestCase {
    private func make(navigate: Bool = false, closedby: String = "closerequest", detail: String = "", detailClosedby: String = "closerequest", bar: Bool = true, heading: Bool = true) throws -> SheetFixture {
        try SheetFixture(page: """
        routes nav
          home "/"
            modal "/modal"
            detail "/detail"

        component Proof
          action open
            nav = push(nav, "/modal")
          action pushDetail
            nav = push(nav, "/detail")
          action cancel
            nav = go(nav, "/")
          action follow(location: string)
            nav = go(nav, location)
          view
            column testId="root" navigationKey=`${top(nav).id}` navigationBack="back"\(navigate ? " navigate=follow" : "") width="100%" height="100%"
              for e in stack(nav) key=e.id
                column navigationKey=`${e.id}` navigationPresentation=(e.name == "modal" ? "modal" : "push") closedby=(e.name == "modal" ? "\(closedby)" : "\(detailClosedby)") testId=e.name navigationBar=(e.name == "detail" ? "\(bar ? "auto" : "none")" : "auto") width=400
                  if e.name == "home"
                    button testId="open" press=open
                      text "Open"
                  if e.name == "modal"
                    header
                      text "Modal" role="heading"
                      button id="back" press=cancel
                        text "Cancel"
                    button testId="push" press=pushDetail
                      text "Push"
                  if e.name == "detail"
                    if \(heading ? "true" : "false")
                      header
                        text "Detail" role="heading"
                    button testId="ask" commandfor="confirm" command="show-modal"
                      text "Ask"
                    \(detail)
              dialog id="confirm" role="alertdialog" aria-label="Continue?"
                text "An alert over the route."
                button commandfor="confirm" command="close" testId="answer"
                  text "OK"
        """)
    }

    func testAncestorExitKeepsThePushedSheetUntilDestroy() throws {
        let f = try make()
        defer { f.close() }
        try f.press("open"); try f.press("push")
        let root = try f.node("root"), detail = try f.node("detail"), window = try XCTUnwrap(detail.window)
        f.presenter.apply(wireBatch([["op": "exit", "id": Int(root.id)]]))
        XCTAssertNil(f.presenter.views[detail.id])
        XCTAssertTrue(f.window.attachedSheet === window)
        XCTAssertTrue(detail.window === window)
        XCTAssertTrue(detail.inert)
        XCTAssertTrue(detail.isAccessibilityHidden())
        f.presenter.apply(wireBatch([]))
        XCTAssertTrue(f.window.attachedSheet === window)
        f.presenter.apply(wireBatch([["op": "destroy", "id": Int(root.id)]]))
        XCTAssertNil(f.window.attachedSheet)
        XCTAssertNil(detail.superview)
    }

    func testHiddenAndHeaderlessNavigationBarsLayOutTheirBackRow() throws {
        for heading in [false, true] {
            for bar in [false, true] {
                let f = try make(bar: bar, heading: heading)
                defer { f.close() }
                try f.press("open"); try f.press("push")
                let chrome = try XCTUnwrap(f.front.contentView as? RouteChrome)
                chrome.layoutSubtreeIfNeeded()
                XCTAssertEqual(chrome.back.isHidden, !bar)
                XCTAssertEqual(chrome.rows, bar ? RouteChrome.titleRow + 1 : 0)
                XCTAssertEqual(chrome.content?.frame.minY, bar ? RouteChrome.titleRow + 1 : 0)
                if bar {
                    XCTAssertGreaterThan(chrome.back.frame.width, 0)
                    XCTAssertGreaterThan(chrome.back.frame.minY, 0)
                    XCTAssertLessThan(chrome.back.frame.maxY, RouteChrome.titleRow)
                    chrome.back.performClick(nil); f.settle()
                    XCTAssertNil(f.presenter.navigationHost.pushedRoute)
                } else { XCTAssertTrue(f.presenter.navigationHost.goBack()) }
            }
        }
    }

    private func key(_ code: UInt16, _ characters: String, _ flags: NSEvent.ModifierFlags = [], in f: SheetFixture) {
        let event = NSEvent.keyEvent(with: .keyDown, location: .zero, modifierFlags: flags, timestamp: 0, windowNumber: f.front.windowNumber,
                                    context: nil, characters: characters, charactersIgnoringModifiers: characters, isARepeat: false, keyCode: code)!
        XCTAssertTrue(f.presenter.routeKey(event, focused: true, in: f.front))
        f.settle()
    }

    private func outside(_ f: SheetFixture) -> Bool {
        var taken = false
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            let event = NSEvent.mouseEvent(with: type, location: NSPoint(x: 20, y: 20), modifierFlags: [], timestamp: 0, windowNumber: f.window.windowNumber,
                                          context: nil, eventNumber: 1, clickCount: 1, pressure: 0)!
            taken = f.presenter.dialogs.pointer(event) || taken
        }
        f.settle()
        return taken
    }

    func testDismissalUsesTheModalBaseAcrossPushAndBack() throws {
        for commandPeriod in [false, true] {
            let f = try make(detailClosedby: "none")
            defer { f.close() }
            try f.press("open"); try f.press("push")
            if commandPeriod { key(47, ".", .command, in: f) } else { key(53, "\u{1b}", in: f) }
            XCTAssertNil(f.window.attachedSheet, "dismissal leaves the whole modal segment")
        }
        let none = try make(closedby: "none")
        defer { none.close() }
        try none.press("open"); try none.press("push")
        let detail = try none.node("detail")
        key(53, "\u{1b}", in: none)
        XCTAssertTrue(none.presenter.dialogs.active === detail)
        key(47, ".", .command, in: none)
        XCTAssertTrue(none.presenter.dialogs.active === detail)
        XCTAssertFalse(outside(none))
        XCTAssertTrue(none.presenter.navigationHost.goBack()); none.settle()
        let modal = try none.node("modal")
        key(53, "\u{1b}", in: none)
        XCTAssertTrue(none.presenter.dialogs.active === modal)
        XCTAssertFalse(outside(none))

        for popFirst in [false, true] {
            let any = try make(closedby: "any", detailClosedby: "none")
            defer { any.close() }
            try any.press("open"); try any.press("push")
            if popFirst { XCTAssertTrue(any.presenter.navigationHost.goBack()); any.settle() }
            XCTAssertTrue(outside(any))
            XCTAssertNil(any.window.attachedSheet)
        }
    }

    func testBackRefusesAnAlertOrAnotherSheetOverAPushedRoute() throws {
        for navigate in [false, true] {
            let f = try make(navigate: navigate)
            defer { f.close() }
            try f.press("open"); try f.press("push")
            let route = try f.node("detail")
            XCTAssertTrue(f.presenter.navigationHost.canGoBack)
            try f.press("ask")
            XCTAssertNotNil(f.presenter.alerts.modal)
            XCTAssertFalse(f.presenter.navigationHost.canGoBack)
            XCTAssertFalse(try XCTUnwrap(f.presenter.navigationHost.agentBack()).refused.isEmpty)
            XCTAssertTrue(f.presenter.navigationHost.pushedRoute?.route === route)
            XCTAssertTrue(f.presenter.alerts.key("Enter")); f.settle()
            let sheet = try XCTUnwrap(route.window), cover = NSWindow()
            sheet.beginSheet(cover)
            XCTAssertFalse(f.presenter.navigationHost.canGoBack)
            XCTAssertFalse(f.presenter.navigationHost.goBack())
            XCTAssertTrue(f.presenter.navigationHost.pushedRoute?.route === route)
            sheet.endSheet(cover)
            let limit = Date().addingTimeInterval(2)
            while sheet.attachedSheet != nil && Date() < limit { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.01)) }
            XCTAssertTrue(f.presenter.navigationHost.goBack()); f.settle()
            XCTAssertNil(f.presenter.navigationHost.pushedRoute)
        }
    }

    func testPoppingExitingChildRouteInsideModalSheetThawsAndShowsModalBase() throws {
        let f = try SheetFixture(page: """
        keyframes fade-out
          to opacity=0

        routes nav
          home "/"
            modal "/modal"
            detail "/detail"

        component Proof
          action open
            nav = push(nav, "/modal")
          action pushDetail
            nav = push(nav, "/detail")
          action popDetail
            nav = go(nav, "/modal")
          action cancel
            nav = go(nav, "/")
          view
            column testId="root" navigationKey=`${top(nav).id}` navigationBack="back" width="100%" height="100%"
              for e in stack(nav) key=e.id
                if e.name == "home"
                  column navigationKey=`${e.id}` testId="home" width=400
                    button testId="open" press=open
                      text "Open"
                if e.name == "modal"
                  column navigationKey=`${e.id}` navigationPresentation="modal" testId="modal" width=400
                    header
                      text "Modal" role="heading"
                      button id="back" press=cancel
                        text "Cancel"
                    button testId="push" press=pushDetail
                      text "Push"
                if e.name == "detail"
                  column navigationKey=`${e.id}` navigationPresentation="push" testId="detail" width=400
                    ::view-transition-old
                      animation="fade-out 60ms linear both"
                    header
                      text "Detail" role="heading"
                    button id="back" testId="pop" press=popDetail
                      text "Back"
        """)
        defer { f.close() }
        try f.press("open")
        try f.press("push")
        let detail = try f.node("detail"), modal = try f.node("modal")
        XCTAssertTrue(f.presenter.dialogs.active === detail)
        try f.press("pop")
        // Wait for the 60ms exit CA animation to finish and resync the sheet.
        waitFor("exiting child route finishes and sheet shows modal base", cap: 5) {
            f.presenter.leaving.isEmpty && f.presenter.dialogs.active === modal
        }
        XCTAssertNotNil(f.window.attachedSheet, "modal sheet remains open")
        XCTAssertTrue(modal.window === f.window.attachedSheet, "modal base is mounted in the sheet")
        XCTAssertFalse(modal.isHidden)
    }

    func testUnmountingNavigationRootClearsRouteTransitionsAndClosesModalSheet() throws {
        // Subcase 1: removing all routes under the root (`found == nil`) clears leaving and closes the sheet.
        let f = try SheetFixture(page: """
        keyframes fade-out
          to opacity=0

        routes nav
          home "/"
            modal "/modal"
            detail "/detail"

        component Proof
          state signedIn = true
          action open
            nav = push(nav, "/modal")
          action pushDetail
            nav = push(nav, "/detail")
          action signOut
            signedIn = false
          action cancel
            nav = go(nav, "/")
          view
            column testId="root" navigationKey=`${top(nav).id}` navigationBack="back" width="100%" height="100%"
              if signedIn
                for e in stack(nav) key=e.id
                  if e.name == "home"
                    column navigationKey=`${e.id}` testId="home" width=400
                      button testId="open" press=open
                        text "Open"
                  if e.name == "modal"
                    column navigationKey=`${e.id}` navigationPresentation="modal" testId="modal" width=400
                      header
                        text "Modal" role="heading"
                        button id="back" press=cancel
                          text "Cancel"
                      button testId="push" press=pushDetail
                        text "Push"
                  if e.name == "detail"
                    column navigationKey=`${e.id}` navigationPresentation="push" testId="detail" width=400
                      ::view-transition-old
                        animation="fade-out 60ms linear both"
                      header
                        text "Detail" role="heading"
                      button testId="signout" press=signOut
                        text "Sign Out"
              if !signedIn
                text "Signed out" testId="signed-out"
        """)
        defer { f.close() }
        try f.press("open")
        try f.press("push")
        XCTAssertNotNil(f.window.attachedSheet)
        try f.press("signout")
        f.settle()
        XCTAssertTrue(f.presenter.leaving.isEmpty, "removing all routes releases presenter.leaving")
        XCTAssertNil(f.presenter.navigationHost.transitions.exiting)
        XCTAssertNil(f.window.attachedSheet, "modal sheet closes rather than wedging open")

        // Subcase 2: destroying the navigation root (`roots.isEmpty`) clears leaving and closes the sheet.
        let g = try SheetFixture(page: """
        keyframes fade-out
          to opacity=0

        routes nav
          home "/"
            modal "/modal"
            detail "/detail"

        component Proof
          action open
            nav = push(nav, "/modal")
          action pushDetail
            nav = push(nav, "/detail")
          action cancel
            nav = go(nav, "/")
          view
            column testId="root" navigationKey=`${top(nav).id}` navigationBack="back" width="100%" height="100%"
              for e in stack(nav) key=e.id
                if e.name == "home"
                  column navigationKey=`${e.id}` testId="home" width=400
                    button testId="open" press=open
                      text "Open"
                if e.name == "modal"
                  column navigationKey=`${e.id}` navigationPresentation="modal" testId="modal" width=400
                    header
                      text "Modal" role="heading"
                      button id="back" press=cancel
                        text "Cancel"
                    button testId="push" press=pushDetail
                      text "Push"
                if e.name == "detail"
                  column navigationKey=`${e.id}` navigationPresentation="push" testId="detail" width=400
                    ::view-transition-old
                      animation="fade-out 60ms linear both"
                    header
                      text "Detail" role="heading"
        """)
        defer { g.close() }
        try g.press("open")
        try g.press("push")
        let root = try g.node("root"), detail = try g.node("detail")
        XCTAssertNotNil(g.window.attachedSheet)
        g.presenter.apply(wireBatch([
            ["op": "destroy", "id": Int(detail.id)],
            ["op": "destroy", "id": Int(root.id)],
            ["op": "roots", "ids": []],
        ]))
        g.settle()
        XCTAssertTrue(g.presenter.leaving.isEmpty, "unmounting the navigation root releases presenter.leaving")
        XCTAssertNil(g.presenter.navigationHost.transitions.exiting)
        XCTAssertNil(g.window.attachedSheet, "modal sheet closes when the navigation root unmounts")
    }

    func testNavigationResetClearsRouteTransitionsState() throws {
        let f = try SheetFixture(page: """
        keyframes fade-in
          from opacity=0
        keyframes fade-out
          to opacity=0

        routes nav
          home "/"
            detail "/detail"

        component Proof
          action pushDetail
            nav = push(nav, "/detail")
          action popDetail
            nav = go(nav, "/")
          view
            column testId="root" navigationKey=`${top(nav).id}` navigationBack="back" width="100%" height="100%"
              for e in stack(nav) key=e.id
                if e.name == "home"
                  column navigationKey=`${e.id}` testId="home" width=400
                    button testId="push" press=pushDetail
                      text "Push"
                if e.name == "detail"
                  column navigationKey=`${e.id}` navigationPresentation="push" testId="detail" width=400
                    ::view-transition-new
                      animation="fade-in 300ms linear both"
                    ::view-transition-old
                      animation="fade-out 300ms linear both"
                    button id="back" press=popDetail
                      text "Back"
        """)
        defer { f.close() }
        try f.press("push")
        let transitions = f.presenter.navigationHost.transitions
        XCTAssertNotNil(transitions.held, "push entrance transition is active")
        f.presenter.reset()
        XCTAssertNil(transitions.held, "reset clears held route transition state")
        XCTAssertNil(transitions.exiting, "reset clears exiting route transition state")
        XCTAssertTrue(f.presenter.leaving.isEmpty)
        // Fresh views with low IDs must not be treated as held/exiting from before reset.
        f.presenter.apply(wireBatch([
            ["op": "create", "id": 1, "kind": "view", "props": ["navigationKey": "fresh", "navigationBack": "back"]],
            ["op": "create", "id": 2, "kind": "view", "props": ["navigationKey": "fresh"]],
            ["op": "children", "id": 1, "ids": [2]], ["op": "roots", "ids": [1]],
        ]))
        let fresh = try XCTUnwrap(f.presenter.views[2])
        XCTAssertFalse(fresh.isHidden)
        XCTAssertFalse(fresh.routeInert)
        XCTAssertFalse(f.presenter.navigationHost.keepsForExit(fresh.id))
    }
}
#endif
