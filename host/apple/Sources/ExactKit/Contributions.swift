// A contribution's identity on every host's ledger of Exact's tagged safe-area writes (LLP 1123 D11,
// "Provenance"): UIKit's (`ContributionsIOS`) and AppKit's (`ContributionsMac`).

/// A contribution's identity on what it writes to, a controller on iOS or a view on AppKit: the
/// declaring node and what it writes, by its index in the kernel's `Operation::ALL`.
package struct ContributionIdentity: Hashable {
    package var node: UInt32
    package var operation: ContributionOperation

    package init(node: UInt32, operation: ContributionOperation) {
        self.node = node
        self.operation = operation
    }
}

/// What a contribution writes, by its index in the kernel's `Operation::ALL`.
package enum ContributionOperation: UInt8 {
    case additionalInsets = 0
    case safeAreaPadding = 1
    case barReservation = 2
    case keyboardInsets = 3
    case containerClearance = 4
    /// The clearance still owed in a native view, through its own controller on iOS and on the view
    /// itself on AppKit (D15).
    case nativeViewInsets = 5
}
