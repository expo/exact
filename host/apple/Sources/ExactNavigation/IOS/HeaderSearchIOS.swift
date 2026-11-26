#if os(iOS) || os(tvOS)
import ExactKit
import UIKit

/// A header's search field as UIKit's search controller (LLP 1075.003
/// §9.6): text, focus and blur go to the authored field's handlers.
final class HeaderSearch: NSObject, UISearchBarDelegate, UISearchControllerDelegate, UITextFieldDelegate, BarSearch {
    let controller = UISearchController(searchResultsController: nil)
    package var shownField: UITextField { controller.searchBar.searchTextField }
    weak var host: NavigationHost?
    /// The field's placeholder is drawn in an authored colour, not UIKit's.
    var paintsPlaceholder = false
    /// The field's `value` the bar last showed: one that differs is the app's write.
    var value: String?
    /// The field's text is drawn in its own colour or the design's FieldText; its fill is the design's Field, not UIKit's capsule.
    var paintsInk = false, paintsGround = false
    /// The scope bar's tabs, and the segment last chosen or set (LLP 1134 D3).
    var scopeTabs: [UInt32] = [], scopeIDs: [String?] = [], scopeSettled: Int?
    weak var scopeList: NodeView?
    /// UIKit's field's text: the node's own colour, else the design's
    /// FieldText where it overrides it, else UIKit's (LLP 1115 D4, LLP 1121
    /// D9); and its fill, the design's Field where it overrides it. Set
    /// again on a live retheme.
    func ink() {
        #if !os(tvOS)
        guard let field else { return }
        let text = controller.searchBar.searchTextField
        // The field's own tint is its node's accent: its text, placeholder and caret in the tint resolve by it
        text.tintColor = field.ownAccent
        // Any `backgroundColor`, nil too, replaces UIKit's capsule (iOS 26, measured): written only where a design names a Field, cleared (unfilled) only where one was.
        if let ground = field.fieldGroundColor { text.backgroundColor = ground; paintsGround = true }
        else if paintsGround { text.backgroundColor = nil; paintsGround = false }
        if field.showsOwnTextColor || ThemeRoles.color("FieldText", in: field) != nil {
            text.textColor = field.fieldInk
            paintsInk = true
        } else if paintsInk {
            text.textColor = .label
            paintsInk = false
        }
        #endif
    }
    weak var field: NodeView? {
        didSet {
            if oldValue !== field { if oldValue?.headerSearch === self { oldValue?.headerSearch = nil }; acceptingDismissal = false }
            #if os(iOS)
            field?.headerSearch = self
            field?.applyFieldDirection()
            controller.searchBar.searchTextField.pasteDelegate = field
            #endif
        }
    }
    private weak var textDelegate: UITextFieldDelegate?
    private var editingCompletion: (value: String, finish: () -> Void)?
    /// The person's close takes UIKit's emptying of the field, the platform's default, to the app.
    private var acceptingDismissal = false
    /// The text as a close began: UIKit empties its bar before editing ends, and a close that keeps the text
    /// commits this instead.
    private var closingText: String?
    /// The host closes the search itself (`close()`) until UIKit has dismissed it: no `cancel`, and the text stays.
    private var closingItself = false
    init(host: NavigationHost) {
        self.host = host
        super.init()
        controller.searchBar.delegate = self
        controller.delegate = self
        #if os(iOS)
        controller.searchResultsUpdater = self
        textDelegate = controller.searchBar.searchTextField.delegate
        controller.searchBar.searchTextField.delegate = self
        #endif
        controller.obscuresBackgroundDuringPresentation = false
        controller.hidesNavigationBarDuringPresentation = true
    }
    /// The navigation item the controller is on.
    weak var item: UINavigationItem?
    /// The field scrolls away with the content only while it is empty
    /// (`NavigationHost.searchField`), and never when its author made it
    /// stick vertically, CSS's way to keep a box in view as its scroller
    /// scrolls (React Navigation's `hideWhenScrolling={false}`): `position:
    /// sticky` with a `top` or `bottom` that is not `auto` (CSS Positioned
    /// Layout 3 §3.4: a box with both `auto` does not stick on that axis).
    func applyHiding() {
        #if os(iOS)
        guard let item else { return }
        let sticks = field.map { f in
            f.layoutPosition == "sticky" && ["top", "bottom"].contains { f.style[$0]?.length(basis: 0) != nil }
        } ?? false
        let hides = (field?.props["value"] ?? "").isEmpty && !sticks
        if item.hidesSearchBarWhenScrolling != hides { item.hidesSearchBarWhenScrolling = hides }
        #endif
    }
    func willPresentSearchController(_ search: UISearchController) {
        acceptingDismissal = false
        presenting = true
        presentation &+= 1
        host?.searchOpenChanged()
    }
    func didPresentSearchController(_ search: UISearchController) {
        #if os(iOS)
        showScopes()
        #endif
        // Inside UIKit's completion of the move its refocus may still come: what the host held back goes after it
        let move = presentation
        if search.transitionCoordinator == nil { presented(move) } else { DispatchQueue.main.async { [weak self] in self?.presented(move) } }
    }
    /// UIKit is moving the bar into search: from `willPresentSearchController` until the turn after
    /// `didPresentSearchController`. The host starts nothing then that ends the field's editing or covers its route
    /// (`resign`, `NavigationHost.searchCovered`), as it starts no push while another runs.
    package private(set) var presenting = false
    /// An end of editing asked for while UIKit moved the bar into search.
    private var resignsWhenPresented = false
    /// Which move into search `presenting` is for: a late end of an earlier one does not end a later one's hold.
    private var presentation = 0
    private func presented(_ move: Int) {
        guard presenting, move == presentation else { return }
        presenting = false
        if resignsWhenPresented {
            resignsWhenPresented = false
            editor.resignFirstResponder()
        }
        host?.searchPresented()
        settle()
    }
    /// Exact's own end of the field's editing: the app's `blur()`, and the host's before UIKit covers the field's
    /// route or as the person goes Back from its results. UIKit makes the field the first responder again at the end
    /// of its move into search when editing ended during it (`-[_UISearchBarVisualProviderIOS
    /// completeTransitionToSearchLayoutState:]`), so an end asked for during that move waits for the move to end.
    /// UIKit's own end during the move (iOS 27.1: its floating bar takes the field from one view to another) is
    /// followed by that refocus, which keeps the field the person's.
    package func resign() {
        if presenting { resignsWhenPresented = true; return }
        editor.resignFirstResponder()
    }
    /// A focus asked for after an end of editing that waits for the move into search: the field stays the person's.
    package func keepEditing() { resignsWhenPresented = false }
    /// The host's own close: the search's settle, a route that no longer holds the field. The text stays.
    func close() {
        guard controller.isActive else { return }
        closingItself = true
        controller.isActive = false
    }
    func willDismissSearchController(_ search: UISearchController) {
        dismissing = true
        if presenting {
            // A close that interrupts the move into search: nothing waits for that move any more
            presenting = false
            resignsWhenPresented = false
            DispatchQueue.main.async { [weak self] in self?.host?.searchPresented() }
        }
        closingText = search.searchBar.text
        // The person's close is HTML's `cancel` at the field; its action's `preventDefault()` keeps the text, as a
        // dialog's keeps it open. Otherwise the close takes UIKit's emptying, the platform's default. A host's own
        // close, a route going and a sheet moving keep the text.
        let persons = !closingItself && editingCompletion == nil && host?.changing == false && host?.presenter.modalHost.inTransition == false
        acceptingDismissal = persons && !cancelKeepsText()
        // Any close leaves the results when they are the route on top, before the app hears the field emptied: a Back
        // from them, unanimated, as the bar's own animation is the one the person sees (an empty search put away
        // included). Under a result opened from them they stay, with the search closed.
        if editingCompletion == nil, let field, let host, host.selectsResults(of: field) {
            host.unanimated = true
            host.goBack()
            if !host.inTransition { host.unanimated = false }
        }
        // The keyboard leaves as the search begins to close, as SwiftUI's `searchable` takes it: closed by its close
        // button, UIKit's integrated bar keeps its field the first responder until the field's view goes at the
        // dismissal's end (iOS 27.1), the keyboard meanwhile showing capitals for the emptied field
        editor.resignFirstResponder()
    }
    func didDismissSearchController(_ search: UISearchController) {
        dismissing = false
        defer { host?.searchOpenChanged() }
        let accepts = acceptingDismissal
        acceptingDismissal = false
        closingItself = false
        let kept = closingText
        closingText = nil
        if accepts {
            let text = search.searchBar.text ?? ""
            searchBar(search.searchBar, textDidChange: text)
            if let field, let presenter = host?.presenter, presenter.views[field.id] === field {
                // Close also commits its reset when the Search key or a drag already ended editing
                presenter.commitEdit(field.id, text, change: field.handlers.contains("change"))
            }
        } else if let field, let value = field.props["value"] ?? kept, search.searchBar.text != value {
            // The field keeps its value; UIKit emptied its bar. Written once the search is closed, so the bar stays
            // closed, holding the text, and shows it again when the person opens it.
            search.searchBar.text = value
            self.value = value
        }
        applyHiding()
        #if os(iOS)
        showScopes()
        #endif
    }
    /// HTML's `cancel` at the field for the person's close; whether its action called `preventDefault()`.
    private func cancelKeepsText() -> Bool {
        guard let field, field.handlers.contains("cancel"), let presenter = host?.presenter, presenter.views[field.id] === field,
              let session = presenter.session else { return false }
        presenter.defaultPrevented = false
        session.applyInput(session.runtime.cancel(field.id, now: session.now()))
        defer { presenter.defaultPrevented = false }
        return presenter.defaultPrevented
    }
    func searchBar(_ bar: UISearchBar, textDidChange text: String) {
        guard let field, let presenter = host?.presenter, presenter.views[field.id] === field else { return }
        field.typedText(text)
        field.applyFieldDirection() // a `direction` that came while it composed (LLP 1001 §1)
    }
    func textField(_ textField: UITextField, shouldChangeCharactersIn range: NSRange, replacementString string: String) -> Bool {
        guard field?.textField(textField, shouldChangeCharactersIn: range, replacementString: string) ?? true else { return false }
        return textDelegate?.textField?(textField, shouldChangeCharactersIn: range, replacementString: string) ?? true
    }
    /// UIKit is closing the search: from `willDismissSearchController` to `didDismissSearchController`.
    private(set) var dismissing = false
    /// The view that edits: the bar's text field (tvOS's bar edits itself).
    var editor: UIResponder {
        #if os(tvOS)
        controller.searchBar
        #else
        controller.searchBar.searchTextField
        #endif
    }
    /// An open search whose field is not editing has something to show, or it closes, keeping its text: text, and its
    /// results route in the router's stack where its field names one (`aria-controls`). An empty field put away closes
    /// into its button, as a person who puts the keyboard away from it means; with text it stays open, as UIKit's and
    /// SwiftUI's do; a Back or a page that took its results away closes it. Asked at rest: after an end of editing
    /// outside a transition, after a sync, a push or a pop, and once the bar has moved into search; not while its
    /// route is covered nor while UIKit moves the split's columns.
    func settle() {
        guard controller.isActive, !presenting, !dismissing, editingCompletion == nil, controller.transitionCoordinator == nil,
              !editor.isFirstResponder,
              let field, let host, host.split?.moving != true, !host.isInactiveRoute(containing: field) else { return }
        let names = !(field.props["accessibilityControls"] ?? "").isEmpty
        guard (controller.searchBar.text ?? "").isEmpty || (names && !host.showsResults(of: field)) else { return }
        close()
    }
    func textFieldShouldClear(_ textField: UITextField) -> Bool {
        guard field?.textFieldShouldClear(textField) ?? true else { return false }
        return textDelegate?.textFieldShouldClear?(textField) ?? true
    }
    func textFieldDidChangeSelection(_ textField: UITextField) {
        textDelegate?.textFieldDidChangeSelection?(textField)
        if let field { host?.presenter.fieldSelections.changed(field) }
        field?.applyFieldDirection() // a `direction` that came while it composed (LLP 1001 §1)
    }
    // UISearchBar remains the recipient of UIKit's focus, return and private callbacks. It is the field's own
    // delegate in the search controller this object holds, so it lives as long as this object, and UIKit's own
    // class answers the same at each call: unlike an app's delegate (`ForwardingDelegate`), it needs no strong hold
    // and no unconditional forwarding.
    override func responds(to selector: Selector!) -> Bool {
        super.responds(to: selector) || (textDelegate?.responds(to: selector) ?? false)
    }
    override func forwardingTarget(for selector: Selector!) -> Any? {
        textDelegate?.responds(to: selector) == true ? textDelegate : nil
    }
    func searchBarTextDidBeginEditing(_ bar: UISearchBar) {
        host?.presenter.searchEditingChanged()
        // UIKit begins the field's editing again while a close it began goes on, after the close took the keys away
        // (`willDismissSearchController`): no focus of the person's, so the app hears none and the keys go again
        if dismissing {
            editor.resignFirstResponder()
            return
        }
        guard let field else { return }
        host?.presenter.fieldSelections.focused(field)
        host?.presenter.focus(field.id)
    }
    /// The keyboard's Search key is this field's Return (LLP 1115.000 D9.2):
    /// its `submit`, then the keyboard goes away, as a hand-built search
    /// bar's delegate resigns it, and the search stays as typed.
    func searchBarSearchButtonClicked(_ bar: UISearchBar) {
        if let field, let presenter = host?.presenter, presenter.views[field.id] === field {
            presenter.commitEdit(field.id, bar.text ?? "", change: field.handlers.contains("change"))
            if field.handlers.contains("submit") { presenter.submit(field.id) }
        }
        resign()
    }
    func searchBarTextDidEndEditing(_ bar: UISearchBar) {
        if controller.transitionCoordinator == nil { DispatchQueue.main.async { [weak self] in self?.settle() } }
        host?.presenter.searchEditingChanged()
        let completion = editingCompletion
        editingCompletion = nil
        if let field, let presenter = host?.presenter, presenter.views[field.id] === field {
            // A close ends editing as it begins, before UIKit empties the bar (`willDismissSearchController`): one that
            // keeps the text commits the text it had, and one that takes the emptying tells the app the emptied field and
            // commits it, `input` before `change`, which leaves the dismissal's end nothing new to tell
            let value = completion?.value ?? (dismissing ? (acceptingDismissal ? "" : closingText) : bar.text) ?? ""
            if completion == nil, dismissing, acceptingDismissal { searchBar(bar, textDidChange: value) }
            presenter.commitEdit(field.id, value, change: field.handlers.contains("change"))
            presenter.blur(field.id)
        }
        if let completion { host?.presenter.afterBatch(completion.finish) }
    }

    /// Keep the authored field alive until UIKit delivers its final edit and blur.
    func endEditing(completion: @escaping () -> Void) -> Bool {
        guard let field, let presenter = host?.presenter, presenter.views[field.id] === field,
              let editor = field.headerField, editor.isEditing else { return false }
        if editingCompletion == nil {
            editingCompletion = (editor.text ?? "", completion)
            controller.isActive = false
            controller.searchBar.endEditing(true)
        }
        return true
    }
}

#if os(iOS)
extension HeaderSearch: UISearchResultsUpdating {
    func updateSearchResults(for search: UISearchController) {
        // UIKit resets the query before ending editing, without the bar's text-change callback
        guard acceptingDismissal else { return }
        searchBar(search.searchBar, textDidChange: search.searchBar.text ?? "")
    }
}
#endif
#endif
