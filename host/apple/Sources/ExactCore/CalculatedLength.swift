import CExact
import CoreGraphics

/// A style owns its environment snapshot; resizing must not grow a global cache.
public final class CalculatedLength: Equatable {
    public let expression: String
    public let environment: [Double]
    private let context: UnsafeMutableRawPointer

    package init?(_ object: [String: BatchValue]) {
        guard let expression = object["calc"]?.string,
              let environment = object["env"]?.numbers,
              let context = Array(expression.utf8).withUnsafeBufferPointer({ text in
                  environment.withUnsafeBufferPointer { exact_calc_create(text.baseAddress, text.count, $0.baseAddress, $0.count) }
              }) else { return nil }
        self.expression = expression
        self.environment = environment
        self.context = context
    }
    /// Interpret the private length wire only in a style, never app payloads: a length, or a coordinate of
    /// `object-position`. A value that holds no calculation keeps its storage, which the decoder shares between the
    /// styles that repeat it (`StyleCache`).
    package static func decode(_ style: NodeStyle) -> NodeStyle {
        var resolved = style
        for (key, value) in style {
            switch value {
            case .object(let object):
                if let calculation = CalculatedLength(object) { resolved[key] = .calculation(calculation) }
            case .array(let coordinates) where coordinates.contains(where: isCalculation):
                resolved[key] = .array(coordinates.map { coordinate in
                    guard case .object(let object) = coordinate, let calculation = CalculatedLength(object) else { return coordinate }
                    return .calculation(calculation)
                })
            default: break
            }
        }
        return resolved
    }
    private static func isCalculation(_ value: BatchValue) -> Bool {
        guard case .object(let object) = value else { return false }
        return object["calc"] != nil
    }
    deinit { exact_calc_destroy(context) }
    public func length(basis: CGFloat) -> CGFloat { CGFloat(exact_calc_resolve(context, Double(basis))) }
    public static func == (a: CalculatedLength, b: CalculatedLength) -> Bool {
        a === b || (a.expression == b.expression && a.environment == b.environment)
    }
    package var value: BatchValue {
        .object(["calc": .string(expression), "env": .array(environment.map(BatchValue.number))])
    }
}
