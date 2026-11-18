// The CSS filter family (LLP 1047.001 D10, the roster's `effects`): a box's
// filtered picture, a presenter's filtered boxes, and the filter engine SVG's
// filters run on too, installed into `BoxFilterLink` by an app whose
// composition links it.
import ExactKit

public enum ExactEffects {
    /// Set `BoxFilterLink`'s slots, before any session is made.
    public static func install() {
        BoxFilterLink.make = { BoxFilter() }
        BoxFilterLink.makeSet = { BoxFilters() }
        BoxFilterLink.prewarm = { SvgFilterMetal.prewarm() }
    }
}
