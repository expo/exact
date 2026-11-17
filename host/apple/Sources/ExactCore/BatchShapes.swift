// Inline values and compound envelope values on the Apple JSON wire. Record
// readers come from kernel/tables/schema.json (LLP 1172.001 E4, §4 step 4).
import Foundation

extension BatchReader {
    /// A `canvas2d` op's lists, `[address, length]` pairs in the runtime's
    /// memory (LLP 1056 D4), copied out now: the runtime keeps them alive
    /// only until its next batch with lists.
    package static func canvasLists(_ value: Any?) -> [Data] {
        (value as? [Any] ?? []).compactMap { pair in
            guard let n = pair as? [Any], n.count == 2, let at = (n[0] as? NSNumber)?.uintValue,
                  let count = (n[1] as? NSNumber)?.intValue, count >= 0,
                  let from = UnsafeRawPointer(bitPattern: at) else { return nil }
            return Data(bytes: from, count: count)
        }
    }

    package mutating func batchModes() throws -> [String: String]? {
        if case .object(let object) = try value() { return object.compactMapValues { $0.string } }
        return nil
    }

    package mutating func batchOrientation() throws -> OrientationRequest? {
        if case .object(let object) = try value() { return OrientationRequest(object) }
        return nil
    }

    package mutating func batchRootTabs() throws -> [RootTabs]? {
        guard case .array(let roots) = try value() else { return nil }
        return roots.compactMap { root in
            guard case .object(let o) = root else { return nil }
            return RootTabs(o)
        }
    }

    package mutating func batchSequence() throws -> (UInt64, UInt64)? {
        let range = try array { try $0.number() }
        if range.count == 2, range.allSatisfy({ $0 >= 0 && $0 == $0.rounded() }) { return (UInt64(range[0]), UInt64(range[1])) }
        return nil
    }

    package mutating func batchOp() throws -> BatchOp {
        let start = offset, startDepth = depth
        try enter(123)
        // Rust writes the discriminator first. JSON object order is not part of
        // the contract: a reordered op's discriminator is found before its
        // fields are read. Only unconverted adapters bridge their payload to Any.
        let first = try string(); try expect(58)
        if first != "op" {
            let fieldsStart = offset
            offset = start
            depth = startDepth
            var discriminator: String?
            try object { r, key in
                if key == "op", discriminator == nil { discriminator = try r.string() }
                else { try r.skip() }
            }
            if discriminator == "surface" {
                offset = start; depth = startDepth
                return try surfaceOp()
            }
            if discriminator == "collections" {
                offset = start
                var op = BatchOp(op: .collections)
                try object { r, key in
                    if key == "items" { op.collections = try r.collectionSnapshots() }
                    else if key == "id" { if !(try r.null()) { op.nodeID = try r.id() } }
                    else { try r.skip() }
                }
                return op
            }
            offset = fieldsStart
            depth = startDepth + 1
            var fields = [first: try value()]
            while !take(125) {
                try expect(44)
                let key = try string(); try expect(58)
                let value = try value()
                if fields[key] == nil { fields[key] = value }
            }
            depth -= 1
            return try BatchFields(fields).op(colorChannels: colorChannels)
        }
        let name = try string()
        var op = BatchOp(op: BatchOp.Kind(rawValue: name) ?? .unknown)
        if op.op == .surface {
            offset = start; depth = startDepth
            return try surfaceOp()
        }
        if op.isAdapter {
            var payload: NodeStyle = ["op": .string(name)]
            while !take(125) {
                try expect(44)
                let key = try string(); try expect(58)
                payload[key] = try value()
            }
            depth -= 1
            op.nodeID = try BatchFields(payload).id("id")
            op.payload = payload.mapValues(\.any)
            if op.op == .canvas2d { op.payload["lists"] = BatchReader.canvasLists(op.payload["lists"]) }
            return op
        }
        let skipsNull = !op.isTypedPayload
        while !take(125) {
            try expect(44)
            let key = try string(); try expect(58)
            if skipsNull, try null() { continue }
            try recordField(&op, key: key)
        }
        depth -= 1
        return op
    }

    /// Surface argument order reaches the module unchanged (LLP 1181 D4),
    /// including records whose fields cross the GPU seam by position.
    private mutating func surfaceOp() throws -> BatchOp {
        var fields: NodeStyle = [:], arguments: Data?
        try object { r, key in
            if key == "values" {
                r.whitespace()
                let start = r.offset
                try r.skip()
                arguments = Data(r.bytes[start..<r.offset])
            } else { fields[key] = try r.value() }
        }
        var op = BatchOp(op: .surface, nodeID: try BatchFields(fields).id("id"))
        op.payload = fields.mapValues(\.any)
        op.payload["values"] = arguments
        return op
    }
}

// Type checks for the uncommon out-of-order op and heterogeneous inline rows.
// Unlike a Decoder, these select a known enum case; none probes by throwing.
package struct BatchFields {
    package let fields: NodeStyle
    package init(_ fields: NodeStyle) { self.fields = fields }
    package func value(_ key: String) -> BatchValue? {
        guard let value = fields[key], value != .null else { return nil }
        return value
    }
    package static func string(_ value: BatchValue) throws -> String {
        guard case .string(let s) = value else { throw BatchReader.Invalid.wire }; return s
    }
    package static func number(_ value: BatchValue) throws -> Double {
        guard case .number(let n) = value else { throw BatchReader.Invalid.wire }; return n
    }
    package static func bool(_ value: BatchValue) throws -> Bool {
        guard case .bool(let b) = value else { throw BatchReader.Invalid.wire }; return b
    }
    package static func id(_ value: BatchValue) throws -> UInt32 {
        guard let id = UInt32(exactly: try number(value)) else { throw BatchReader.Invalid.wire }; return id
    }
    package static func object(_ value: BatchValue) throws -> NodeStyle {
        guard case .object(let o) = value else { throw BatchReader.Invalid.wire }; return o
    }
    package static func array<T>(_ value: BatchValue, _ element: (BatchValue) throws -> T) throws -> [T] {
        guard case .array(let a) = value else { throw BatchReader.Invalid.wire }; return try a.map(element)
    }
    package func string(_ key: String) throws -> String? { try value(key).map(Self.string) }
    package func number(_ key: String) throws -> Double? { try value(key).map(Self.number) }
    package func id(_ key: String) throws -> UInt32? { try value(key).map(Self.id) }
    package func bool(_ key: String) throws -> Bool? { try value(key).map(Self.bool) }
    package func object(_ key: String) throws -> NodeStyle { try value(key).map(Self.object) ?? [:] }
    package func strings(_ key: String) throws -> [String: String] { try object(key).mapValues(Self.string) }
    package func array<T>(_ key: String, _ element: (BatchValue) throws -> T) throws -> [T] {
        try value(key).map { try Self.array($0, element) } ?? []
    }
}

extension InlineStyle {
    package mutating func color(_ value: BatchValue, foreground: Bool, channels: (BatchValue, Bool) -> [Double]?) throws {
        if foreground { fixedInk = value.fixesInk }
        if value.isWideColor {
            // @ref LLP 1100 D2
            guard let light = channels(value, false), let dark = channels(value, true) else { throw BatchReader.Invalid.wire }
            paired = value.isSchemeColor
            if foreground { run.color = light; darkColor = dark } else { run.background = light; darkBackground = dark }
        } else if value.isSystemColor {
            // @ref LLP 1095 D5 — a platform colour, kept by name and resolved
            // as the run is read, in its paragraph owner's traits (an inline
            // run has no view), so a contrast or level change reaches it.
            guard channels(value, false) != nil, channels(value, true) != nil else { throw BatchReader.Invalid.wire }
            paired = true
            if foreground { colorRef = value } else { backgroundRef = value }
        } else if foreground {
            guard case .array(let a) = value else { throw BatchReader.Invalid.wire }
            if a.count == 2 {
                paired = true
                run.color = try BatchFields.array(a[0], BatchFields.number)
                darkColor = try BatchFields.array(a[1], BatchFields.number)
            } else {
                run.color = try a.map(BatchFields.number); darkColor = run.color
            }
            guard run.color?.count == 4, darkColor?.count == 4 else { throw BatchReader.Invalid.wire }
        } else {
            guard case .array(let a) = value else { throw BatchReader.Invalid.wire }
            if a.count == 2 {
                paired = true
                run.background = try BatchFields.array(a[0], BatchFields.number)
                darkBackground = try BatchFields.array(a[1], BatchFields.number)
            } else {
                run.background = try a.map(BatchFields.number); darkBackground = run.background
            }
            guard run.background?.count == 4, darkBackground?.count == 4 else { throw BatchReader.Invalid.wire }
        }
    }
    package mutating func height(_ value: BatchValue?) throws {
        guard let value, value != .null else { return }
        switch value {
        case .number(let ratio): run.lineHeight = CGFloat(Float(ratio) * Float(run.size))
        case .string(let s):
            if s.hasSuffix("px"), let px = Float(s.dropLast(2)) { run.lineHeight = CGFloat(px) }
        default: throw BatchReader.Invalid.wire
        }
    }
}

extension BatchValue {
    package var adapterRank: Int64? { (any as? NSNumber)?.int64Value }
    package var adapterString: String? { any as? String }
    package var adapterBool: Bool? { any as? Bool }
}

// Foundation's numeric adapter casts accept booleans; a field's `[Double]`
// rectangle does not. Keep those wire semantics (LLP 1172.001 §4 step 4).
extension BatchValue {
    package var adapterNumber: Double? {
        switch self {
        case .number(let n): return n
        case .bool(let b): return b ? 1 : 0
        default: return nil
        }
    }
    package var adapterID: UInt32? { adapterNumber.map { NSNumber(value: $0).uint32Value } }
    package var adapterIDs: [UInt32]? {
        guard let values = array else { return nil }
        let ids = values.compactMap(\.adapterID)
        return ids.count == values.count ? ids : nil
    }
    package var adapterPadding: [CGFloat?]? {
        guard let values = array, values.count == 4 else { return nil }
        return values.map { $0.adapterNumber.map { CGFloat($0) } }
    }
    package var adapterRect: CGRect? { Self.rectangle(numbers) }
    package var adapterNumberRect: CGRect? {
        guard let values = array else { return nil }
        let numbers = values.compactMap(\.adapterNumber)
        return numbers.count == values.count ? Self.rectangle(numbers) : nil
    }
    private static func rectangle(_ values: [Double]?) -> CGRect? {
        guard let values, values.count == 4 else { return nil }
        return CGRect(x: values[0], y: values[1], width: values[2], height: values[3])
    }
}
