#if os(iOS) || os(tvOS)
import ExactKit
import UIKit

/// A header's search field as UIKit's search controller (LLP 1075.003
/// §9.6): text, focus and blur go to the authored field's handlers.
final class HeaderSearch: NSObject, UISearchBarDelegate, UISearchControllerDelegate, UITextFieldDelegate, BarSearch {
    let controller: UISearchController
    var paired: Bool {
        #if os(iOS)
        results != nil
        #else
        false
        #endif
    }
    #if os(iOS)
    let results: HeaderSearchResults?
    let resultsContainer: UIViewController?
    var placesResultsInDetail: Bool { resultsContainer != nil }
    private(set) var open = false
    #if os(iOS)
    /// The snapshot over N1's swap, until the search's move begins (`swapAlongTransition`)
    private var fading: UIView?
    #endif
    /// Whether the results hold the detail column: from the search's presentation until the person's close begins, so
    /// the detail they covered comes back as UIKit dismisses the search (LLP 1127.000 D2). The host's own close keeps
    /// them there until it settles.
    var resultsHoldDetail: Bool { open && pairedClose.map { $0.cause == .projection } ?? true }
    private var restoreActivation = false
    private var restoreEditing = false
    private var savedSelection: (start: Int, end: Int)?
    var hasDeferredPostureRestore: Bool { restoreActivation || restoreEditing || savedSelection != nil }
    package var shownField: UITextField { bindEditor() }
    #endif
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
            _ = bindEditor()
            field?.applyFieldDirection()
            #endif
        }
    }
    private weak var textDelegate: UITextFieldDelegate?
    #if os(iOS)
    private weak var boundEditor: UISearchTextField?
    enum CloseCause { case projection, agent, cancelButton, uikit }
    private struct Close {
        let presentation: Int
        let queryRevision: Int
        let query: String
        let cause: CloseCause
        let projection: UInt64
    }
    private var pairedClose: Close?
    private var queryRevision = 0
    #endif
    private var editingCompletion: (value: String, finish: () -> Void)?
    /// The person's close takes UIKit's emptying of the field, the platform's default, to the app.
    private var acceptingDismissal = false
    /// The text as a close began: UIKit empties its bar before editing ends, and a close that keeps the text
    /// commits this instead.
    private var closingText: String?
    /// The host closes the search itself (`close()`) until UIKit has dismissed it: no `cancel`, and the text stays.
    private var closingItself = false
    init(host: NavigationHost, resultsOwner: RouteController? = nil, detailPlacement: Bool = false) {
        self.host = host
        #if os(iOS)
        let results = resultsOwner.map { HeaderSearchResults(host: host, owner: $0) }
        self.results = results
        let container = detailPlacement && results != nil ? UIViewController() : nil
        resultsContainer = container
        controller = SearchControllerIOS(searchResultsController: container ?? results)
        #else
        controller = UISearchController(searchResultsController: nil)
        #endif
        super.init()
        controller.searchBar.delegate = self
        controller.delegate = self
        #if os(iOS)
        controller.searchResultsUpdater = self
        (controller as? SearchControllerIOS)?.willMove = { [weak self] transition in self?.fade(along: transition) }
        _ = bindEditor()
        if results != nil { controller.showsSearchResultsController = false }
        #endif
        controller.obscuresBackgroundDuringPresentation = false
        if !paired { controller.hidesNavigationBarDuringPresentation = true }
    }

    #if os(iOS)
    @discardableResult
    func bindEditor() -> UISearchTextField {
        let editor = controller.searchBar.searchTextField
        if boundEditor !== editor || editor.delegate !== self {
            boundEditor = editor
            if editor.delegate !== self { textDelegate = editor.delegate; editor.delegate = self }
        }
        if editor.pasteDelegate !== field { editor.pasteDelegate = field }
        if editor.accessibilityIdentifier != field?.props["testId"] { editor.accessibilityIdentifier = field?.props["testId"] }
        if editor.accessibilityLabel != field?.props["accessibilityLabel"] { editor.accessibilityLabel = field?.props["accessibilityLabel"] }
        return editor
    }

    func bindResults(in owner: RouteController, field: NodeView) {
        guard let results, let host else { return }
        results.bind(host.searchResult(named: field.props["accessibilityControls"], in: owner.node))
        results.update(self)
    }

    func setValue(_ query: String) {
        if value != query { queryRevision += 1 }
        let bar = controller.searchBar
        let editor = bindEditor()
        let composing = editor.markedTextRange != nil
        if !dismissing && bar.text != query && (!editor.isFirstResponder || (value != query && !composing)) { bar.text = query }
        if !composing { value = query }
    }

    private func beginClose(_ cause: CloseCause) {
        guard pairedClose == nil else { return } // The first report owns this presentation's close
        pairedClose = Close(presentation: presentation, queryRevision: queryRevision, query: field?.props["value"] ?? "",
                            cause: cause, projection: host?.split?.searchProjectionEpoch ?? 0)
        dismissing = true
        presenting = false
        resignsWhenPresented = false
        host?.scheduleSplitProjection()
    }

    func closeFromAgent() {
        guard controller.isActive || open else { return }
        beginClose(.agent)
        controller.isActive = false
    }

    func settlePairedClose() {
        guard let close = pairedClose, !controller.isActive, controller.transitionCoordinator == nil else { return }
        pairedClose = nil
        dismissing = false
        guard let host, let field, host.presenter.views[field.id] === field, close.presentation == presentation else { return }
        // An integrated X and an automatic dismissal use the same callback. A move of the owner
        // disqualifies that unattributed dismissal even if the fold or cover has since reversed.
        let userClose = close.cause == .agent || close.cause == .cancelButton
            || (close.cause == .uikit && host.exposesSearchOwner(self)
            && close.projection == host.split?.searchProjectionEpoch)
        if userClose {
            endDetailSession()
            let keep = cancelKeepsText()
            if !keep, !close.query.isEmpty, queryRevision == close.queryRevision, presentation == close.presentation,
               !controller.isActive, self.field === field, host.presenter.views[field.id] === field,
               field.props["value"] == close.query {
                field.typedText("")
                host.presenter.commitEdit(field.id, "", change: field.handlers.contains("change"))
            }
        }
        if presentation == close.presentation, !controller.isActive, self.field === field, host.presenter.views[field.id] === field {
            setValue(field.props["value"] ?? "")
            applyHiding()
        }
    }

    func searchBarCancelButtonClicked(_ searchBar: UISearchBar) {
        if paired { beginClose(.cancelButton) }
        controller.isActive = false
    }

    private func endDetailSession() {
        guard placesResultsInDetail else { return }
        open = false
        restoreActivation = false
        discardPostureEditing()
        if let split = host?.split, split.pairedSearch === self {
            split.searchState.resultBase = nil
            split.searchState.retainedResultBase = nil
        }
    }

    func discardPostureEditing() {
        restoreEditing = false
        savedSelection = nil
    }

    func prepareCoverMove() {
        guard placesResultsInDetail, open, pairedClose == nil else { return }
        restoreActivation = true
    }

    func preparePostureMove() {
        prepareCoverMove()
        // One move can report both a width change and a display-mode change: its first report keeps the field's state.
        guard restoreActivation, !restoreEditing, savedSelection == nil, host?.split?.searchState.resultBase == nil else { return }
        let editor = bindEditor()
        // UIKit can take the field out of the window, and so out of editing, before it reports the move, and gives it
        // editing back itself after the move: the range is kept either way, and Exact restores only editing it saw.
        restoreEditing = editor.isFirstResponder
        guard editor.markedTextRange == nil, let range = editor.selectedTextRange else { return }
        savedSelection = (editor.offset(from: editor.beginningOfDocument, to: range.start),
                          editor.offset(from: editor.beginningOfDocument, to: range.end))
    }

    func restoreActivationAfterPosture() {
        guard restoreActivation, open, pairedClose == nil, host?.exposesSearchOwner(self) == true,
              item?.searchController === controller else { return }
        restoreActivation = false
        if !controller.isActive {
            if !restoreEditing { resignsWhenPresented = true }
            controller.isActive = true
        }
        if let field { setValue(field.props["value"] ?? "") }
    }

    func restoreEditingAfterPosture() {
        guard restoreEditing || savedSelection != nil, !restoreActivation, open, pairedClose == nil, controller.isActive, !presenting,
              let host, host.exposesSearchOwner(self) else { return }
        let editor = bindEditor()
        if let other = host.presenter.keyboardEditor, other !== editor {
            discardPostureEditing()
            return
        }
        guard host.presenter.viewport.window?.isKeyWindow == true else { return }
        guard editor.window != nil, !editor.bounds.isEmpty,
              sequence(first: editor as UIView, next: \.superview).allSatisfy({ !$0.isHidden && $0.alpha > 0.01 }) else {
            host.scheduleSplitProjection()
            return
        }
        guard editor.isFirstResponder else {
            // Drop the range of a field that was not editing and that the move gave no editing back
            guard restoreEditing else { return discardPostureEditing() }
            // Give UIKit's editing callbacks and any ensuing projection a turn before restoring the range.
            if editor.becomeFirstResponder() { host.scheduleSplitProjection() }
            return
        }
        let selection = savedSelection
        discardPostureEditing()
        if let selection, editor.markedTextRange == nil,
           let start = editor.position(from: editor.beginningOfDocument, offset: selection.start),
           let end = editor.position(from: editor.beginningOfDocument, offset: selection.end) {
            editor.selectedTextRange = editor.textRange(from: start, to: end)
        }
    }

    func reconcilePresentation() {
        if presenting, !controller.isBeingPresented, controller.transitionCoordinator == nil {
            presented(presentation)
        }
    }

    func returnAccessibilityFocus() {
        guard let host, let field else { return }
        host.presenter.afterBatch { [weak self, weak field] in
            guard let self, let field, self.field === field, let host = self.host, host.splitPrimaryExposed else { return }
            let editor = self.bindEditor()
            if editor.window != nil && !editor.isHidden {
                if !self.placesResultsInDetail { host.presenter.focus(field.id) }
                UIAccessibility.post(notification: .layoutChanged, argument: editor)
            }
        }
    }
    #endif

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
        #if os(iOS)
        if paired {
            // Reopening supersedes an older dismissal, including its final text reset.
            pairedClose = nil
            dismissing = false
            if placesResultsInDetail { open = true }
            if let field { setValue(field.props["value"] ?? "") }
            if !placesResultsInDetail || host?.split?.searchState.resultBase == nil && host?.split?.searchState.ownerMoving != true {
                host?.searchInteracted(self)
            }
            results?.update(self)
            host?.scheduleSplitProjection()
            swapAlongTransition()
        }
        #endif
    }
    #if os(iOS)
    /// The results come into the detail column, or give it back, as the delegate announces the search's move, under a
    /// snapshot that fades out on the move's own coordinator (`fade(along:)`), or goes as the move ends without one
    /// (LLP 1127.000 D2).
    private func swapAlongTransition() {
        fading?.removeFromSuperview()
        fading = nil
        guard placesResultsInDetail, let split = host?.split, split.pairedSearch === self else { return }
        fading = split.swapResultsWithSearch()
    }

    /// Fades the snapshot over the swap out alongside the search's move, as UIKit begins it and an animation can still
    /// join it (`SearchControllerIOS`)
    private func fade(along transition: UIViewControllerTransitionCoordinator?) {
        guard let cover = fading else { return }
        fading = nil
        guard let split = host?.split else { return cover.removeFromSuperview() }
        split.fadeOut(cover, along: transition)
    }
    #endif
    func didPresentSearchController(_ search: UISearchController) {
        #if os(iOS)
        showScopes()
        #endif
        #if os(iOS)
        fade(along: nil)
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
        #if os(iOS)
        _ = bindEditor()
        if paired { host?.scheduleSplitProjection() }
        else { host?.searchPresented() }
        #else
        host?.searchPresented()
        #endif
        settle()
    }
    /// Exact's own end of the field's editing: the app's `blur()`, and the host's before UIKit covers the field's
    /// route. UIKit can restore first responder at the end of its presentation,
    /// so an end asked for during that move waits for the move to end.
    /// UIKit's own end during the move (iOS 27.1: its floating bar takes the field from one view to another) is
    /// followed by that refocus, which keeps the field the person's.
    package func resign() {
        #if os(iOS)
        discardPostureEditing()
        #endif
        if presenting { resignsWhenPresented = true; return }
        editor.resignFirstResponder()
    }
    /// A focus asked for after an end of editing that waits for the move into search: the field stays the person's.
    package func keepEditing() { resignsWhenPresented = false }
    /// The host's own close: the search's settle, a route that no longer holds the field. The text stays.
    func close() {
        #if os(iOS)
        if paired {
            let wasOpen = open
            endDetailSession()
            guard controller.isActive || wasOpen else { return }
            beginClose(.projection)
            controller.isActive = false
            return
        }
        #endif
        guard controller.isActive else { return }
        closingItself = true
        controller.isActive = false
    }
    func willDismissSearchController(_ search: UISearchController) {
        #if os(iOS)
        if paired {
            let exposed = host?.mayCancelSearch(self) == true
            if !exposed { prepareCoverMove() }
            beginClose(exposed ? .uikit : .projection)
            swapAlongTransition()
            return
        }
        #endif
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
        // The keyboard leaves as the search begins to close, as SwiftUI's `searchable` takes it: closed by its close
        // button, UIKit's integrated bar keeps its field the first responder until the field's view goes at the
        // dismissal's end (iOS 27.1), the keyboard meanwhile showing capitals for the emptied field
        editor.resignFirstResponder()
    }
    func didDismissSearchController(_ search: UISearchController) {
        #if os(iOS)
        fade(along: nil)
        if paired {
            host?.scheduleSplitProjection()
            return
        }
        #endif
        dismissing = false
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
        guard let field, field.handlers.contains("cancel"), let presenter = host?.presenter,
              presenter.views[field.id] === field else { return false }
        return presenter.searchCancelPreventsReset(field.id)
    }
    func searchBar(_ bar: UISearchBar, textDidChange text: String) {
        guard let field, let presenter = host?.presenter, presenter.views[field.id] === field else { return }
        #if os(iOS)
        if paired {
            guard controller.isActive, pairedClose == nil,
                  !placesResultsInDetail || host?.split?.searchState.ownerMoving != true else { return }
            queryRevision += 1
            host?.searchInteracted(self, editing: true)
        }
        #endif
        field.typedText(text)
        field.applyFieldDirection() // a `direction` that came while it composed (LLP 1001 §1)
    }
    func textField(_ textField: UITextField, shouldChangeCharactersIn range: NSRange, replacementString string: String) -> Bool {
        guard field?.textField(textField, shouldChangeCharactersIn: range, replacementString: string) ?? true else { return false }
        return textDelegate?.textField?(textField, shouldChangeCharactersIn: range, replacementString: string) ?? true
    }
    /// Paired search holds its projection until the closing presentation is
    /// inactive and its transition has settled. Ordinary search follows UIKit's dismissal callbacks.
    private(set) var dismissing = false
    /// The view that edits: the bar's text field (tvOS's bar edits itself).
    var editor: UIResponder {
        #if os(tvOS)
        controller.searchBar
        #else
        controller.searchBar.searchTextField
        #endif
    }
    func settle() {
        #if os(iOS)
        _ = bindEditor()
        if paired {
            results?.update(self)
            return
        }
        #endif
        guard controller.isActive, !presenting, !dismissing, editingCompletion == nil, controller.transitionCoordinator == nil,
              !editor.isFirstResponder, (controller.searchBar.text ?? "").isEmpty,
              let field, let host, host.split?.moving != true, !host.isInactiveRoute(containing: field) else { return }
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
    // UISearchBar remains the recipient of UIKit's focus and return callbacks. It is the field's own
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
        #if os(iOS)
        _ = bindEditor()
        #endif
        host?.presenter.searchEditingChanged()
        // UIKit begins the field's editing again while a close it began goes on, after the close took the keys away
        // (`willDismissSearchController`): no focus of the person's, so the app hears none and the keys go again
        if dismissing {
            if !paired { editor.resignFirstResponder() }
            return
        }
        guard let field else { return }
        #if os(iOS)
        if paired { host?.searchInteracted(self, editing: true) }
        #endif
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
            let closingValue = paired ? field.props["value"] : (acceptingDismissal ? "" : closingText)
            #if os(iOS)
            let moving = placesResultsInDetail && host?.split?.searchState.ownerMoving == true
            #else
            let moving = false
            #endif
            let value = completion?.value ?? (dismissing || moving ? closingValue : bar.text) ?? ""
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
            close()
            controller.searchBar.endEditing(true)
        }
        return true
    }
}

#if os(iOS)
extension HeaderSearch: UISearchResultsUpdating {
    func updateSearchResults(for search: UISearchController) {
        _ = bindEditor()
        if paired {
            results?.update(self)
            return
        }
        // UIKit resets the query before ending editing, without the bar's text-change callback
        guard acceptingDismissal else { return }
        searchBar(search.searchBar, textDidChange: search.searchBar.text ?? "")
    }
}
#endif
#if os(iOS)
/// UIKit's search controller, which tells its owner as UIKit begins to present or dismiss it, with the move's
/// coordinator, while an animation can still be queued in its context: the delegate's `willPresentSearchController(_:)`
/// and `willDismissSearchController(_:)` come before UIKit names the coordinator, and a turn later
/// `animateAlongsideTransition(in:animation:completion:)` returns false.
final class SearchControllerIOS: UISearchController {
    var willMove: ((UIViewControllerTransitionCoordinator?) -> Void)?

    override func viewWillAppear(_ animated: Bool) {
        super.viewWillAppear(animated)
        willMove?(transitionCoordinator)
    }

    override func viewWillDisappear(_ animated: Bool) {
        super.viewWillDisappear(animated)
        willMove?(transitionCoordinator)
    }
}
#endif
#endif
