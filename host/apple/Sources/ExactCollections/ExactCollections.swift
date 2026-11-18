// The lists capability (LLP 1047.001 D10; the roster's `Collections`): a
// windowed list, a row's swipe actions and a list's native row cells. An
// app's composition links this module only when its plan has one of them
// (a windowed list, a swipe row, a reorder group or grip, a list whose rows
// are cells), and installs it before any session is made.
import ExactKit
import ExactCore
import CExact

public enum ExactCollections {
    public static func install() {
        ListLink.make = { CollectionHost($0) }
        #if os(iOS) || os(tvOS)
        SwipeActionsLink.make = { SwipeActionsHost($0) }
        SwipeActionsLink.makeRowCells = { RowCells($0) }
        #endif
        CoreExports.collectionFeedback = exact_collection_feedback
        CoreExports.listCovered = exact_list_covered
        CoreExports.listIndex = exact_list_index
        CoreExports.listText = exact_list_text
        CoreExports.collectionFixedSize = exact_collection_fixed_size
        CoreExports.collectionSnap = exact_collection_snap
        CoreExports.collectionSnapStop = exact_collection_snap_stop
    }
}
