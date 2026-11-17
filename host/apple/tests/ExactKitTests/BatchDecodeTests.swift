import Foundation
import XCTest
@testable import ExactKit
#if os(macOS)
import AppKit
#endif

// Existing JSON-building fixtures keep their call sites and go through the
// production decoder. OracleBatch has its own, unshadowed Decodable entry.
extension JSONDecoder {
    func decode(_ type: NodeStyle.Type, from data: Data) throws -> NodeStyle {
        try data.withUnsafeBytes {
            var reader = BatchReader(bytes: $0.bindMemory(to: UInt8.self))
            let style = try CalculatedLength.decode(reader.values())
            try reader.end()
            return style
        }
    }
    func decode(_ type: Batch.Type, from data: Data) throws -> Batch {
        let batch = Batch.decode(data)
        guard batch.error != "unreadable batch" else { throw BatchReader.Invalid.wire }
        return batch
    }
    func decode(_ type: [InlineText].Type, from data: Data) throws -> [InlineText] {
        try data.withUnsafeBytes {
            var reader = BatchReader(bytes: $0.bindMemory(to: UInt8.self))
            let rows = try reader.array { try $0.inline() }
            try reader.end()
            return rows
        }
    }
}

final class BatchDecodeTests: XCTestCase {
    private var compared = 0
    private var operations = 0
    private var paragraphs = 0

    @discardableResult
    func compare(_ data: Data, decoded: Batch? = nil, resolver: AssetResolver? = nil,
                 file: StaticString = #filePath, line: UInt = #line) throws -> Batch {
        let decoder = JSONDecoder()
        decoder.userInfo[.oracleAssets] = resolver
        let old = try decoder.decode(OracleBatch.self, from: data)
        let metadata = try decoder.decode(OracleBatchMetadata.self, from: data)
        let new = decoded ?? Batch.decode(data, resolver: resolver)
        XCTAssertEqual(new.error, old.error, file: file, line: line)
        XCTAssertEqual(new.timers, old.timers, file: file, line: line)
        XCTAssertEqual(new.motion, old.motion, file: file, line: line)
        XCTAssertEqual(new.pending, old.pending, file: file, line: line)
        XCTAssertEqual(new.clock?.bitPattern, old.clock?.bitPattern, file: file, line: line)
        XCTAssertEqual(new.timerDueMs?.bitPattern, old.timerDueMs?.bitPattern, file: file, line: line)
        XCTAssertEqual([new.canvas, new.waitContinue, new.frames, new.canvasOwed, new.controls, new.layoutProvisional,
                        new.unchanged, new.storeReadPending, new.storeActivationPending, new.busy, new.requests, new.spatial,
                        new.landing, new.stored, new.unsupported, new.facts],
                       [metadata.canvas, metadata.waitContinue, metadata.frames, metadata.canvasOwed, metadata.controls, metadata.layoutProvisional,
                        metadata.unchanged, metadata.storeReadPending, metadata.storeActivationPending, metadata.busy, metadata.requests, metadata.spatial,
                        metadata.landing, metadata.stored, metadata.unsupported, metadata.facts].map { $0 ?? false }, file: file, line: line)
        XCTAssertEqual(new.wakelock, metadata.wakelock, file: file, line: line)
        XCTAssertEqual(new.mustFinish, metadata.mustFinish, file: file, line: line)
        XCTAssertEqual(new.timerWaits, metadata.waits ?? [], file: file, line: line)
        XCTAssertEqual([new.canvasImages, new.alerts, new.journal, new.announced],
                       [metadata.canvasImages, metadata.alerts, metadata.journal, metadata.announced].map { $0 ?? [] }, file: file, line: line)
        XCTAssertEqual(new.live, metadata.live, file: file, line: line)
        XCTAssertEqual(new.design, metadata.design?.string, file: file, line: line)
        var modes: [String: String]?
        if case .object(let values)? = metadata.modes { modes = values.compactMapValues { $0.string } }
        XCTAssertEqual(new.modes, modes, file: file, line: line)
        let sequence = metadata.seq.flatMap { range -> [UInt64]? in
            guard range.count == 2, range.allSatisfy({ $0 >= 0 && $0 == $0.rounded() }) else { return nil }
            return range.map { UInt64($0) }
        }
        XCTAssertEqual(new.seq.map { [$0.0, $0.1] }, sequence, file: file, line: line)
        XCTAssertEqual(new.nativeTimers, metadata.nativeTimers?.map {
            NativeTimerWake(index: $0.index, generation: UInt64($0.generation)!, due: $0.due,
                            interval: $0.interval, tolerance: $0.tolerance, once: $0.once)
        }, file: file, line: line)
        XCTAssertEqual(new.ops.count, old.ops.count, file: file, line: line)
        for (a, b) in zip(new.ops, old.ops) {
            XCTAssertEqual(a.op.rawValue, b.op.rawValue, file: file, line: line)
            XCTAssertEqual(a.nodeID, b.nodeID, file: file, line: line)
            XCTAssertEqual(a.kind, b.kind, file: file, line: line)
            XCTAssertEqual(a.props, b.props, file: file, line: line)
            XCTAssertEqual(a.clear, b.clear, file: file, line: line)
            XCTAssertEqual(a.handlers, b.handlers, file: file, line: line)
            XCTAssertEqual(a.ids, b.ids, file: file, line: line)
            XCTAssertEqual(a.property, b.property, file: file, line: line)
            XCTAssertEqual([a.x, a.y, a.w, a.h].map(\.bitPattern), [b.x, b.y, b.w, b.h].map(\.bitPattern), file: file, line: line)
            XCTAssertEqual(a.fitHeight, b.fitHeight, file: file, line: line)
            XCTAssertEqual(a.style, b.style.mapValues(convert), file: file, line: line)
            // A canvas's lists cross as `[address, length]` in the runtime's
            // memory, which the reader copies out (still valid here).
            var expected = b.payload
            if a.op == .canvas2d { expected["lists"] = BatchReader.canvasLists(expected["lists"]) }
            if a.op == .rank {
                XCTAssertEqual(a.rank, (expected["rank"] as? NSNumber)?.int64Value, file: file, line: line)
                XCTAssertTrue(a.payload.isEmpty, file: file, line: line)
            } else if a.op == .title {
                XCTAssertEqual(a.title, expected["title"] as? String, file: file, line: line)
                XCTAssertEqual(a.edited, expected["edited"] as? Bool, file: file, line: line)
                XCTAssertTrue(a.payload.isEmpty, file: file, line: line)
            } else if a.op == .fieldContent {
                XCTAssertEqual(a.contentRect, b.contentRect, file: file, line: line)
                XCTAssertEqual(a.contentPadding, b.contentPadding, file: file, line: line)
                XCTAssertEqual(a.fieldCell, expected["cell"] as? Bool, file: file, line: line)
                XCTAssertTrue(a.payload.isEmpty, file: file, line: line)
            } else if a.op == .layoutHits {
                XCTAssertEqual(a.layoutBoxes, b.layoutBoxes, file: file, line: line)
                XCTAssertTrue(a.payload.isEmpty, file: file, line: line)
            } else if a.op == .layoutHitsWithdrawn {
                XCTAssertEqual(a.withdrawnIDs, (expected["ids"] as? [NSNumber])?.map(\.uint32Value), file: file, line: line)
                XCTAssertTrue(a.payload.isEmpty, file: file, line: line)
            } else if a.op == .layoutPromote {
                XCTAssertEqual(a.layoutParent, (expected["parent"] as? NSNumber)?.uint32Value, file: file, line: line)
                XCTAssertEqual(a.layoutFrame, b.layoutFrame, file: file, line: line)
                XCTAssertEqual(a.layoutKeep, (expected["keep"] as? [NSNumber])?.map(\.uint32Value), file: file, line: line)
                XCTAssertEqual(a.layoutChildren, b.layoutChildren, file: file, line: line)
                XCTAssertTrue(a.payload.isEmpty, file: file, line: line)
            } else if a.op == .collections {
                let items = expected["items"] as? [[String: Any]]
                let expectedSnapshots = (items ?? []).compactMap(OracleCollectionSnapshot.init)
                let count = items?.count == expectedSnapshots.count ? expectedSnapshots.count : nil
                XCTAssertTrue(a.payload.isEmpty, "snapshots do not bridge through Any", file: file, line: line)
                XCTAssertEqual(a.collections?.count, count, file: file, line: line)
                for (snapshot, oracle) in zip(a.collections ?? [], expectedSnapshots) {
                    XCTAssertEqual(snapshot.view, oracle.view, file: file, line: line)
                    XCTAssertEqual(snapshot.horizontal, oracle.horizontal, file: file, line: line)
                    XCTAssertEqual(snapshot.parent, oracle.parent, file: file, line: line)
                    XCTAssertEqual(snapshot.revision, oracle.revision, file: file, line: line)
                    XCTAssertEqual(snapshot.sequence, oracle.sequence, file: file, line: line)
                    XCTAssertEqual(snapshot.extent, oracle.extent, file: file, line: line)
                    XCTAssertEqual(snapshot.count, oracle.count, file: file, line: line)
                    XCTAssertEqual(snapshot.fixed.map(\.view), oracle.fixed.map(\.view), file: file, line: line)
                    XCTAssertEqual(snapshot.fixed.map(\.roots), oracle.fixed.map(\.roots), file: file, line: line)
                    XCTAssertEqual(snapshot.fixed.map(\.size), oracle.fixed.map(\.size), file: file, line: line)
                    XCTAssertEqual(snapshot.fixed.map(\.epoch), oracle.fixed.map(\.epoch), file: file, line: line)
                    XCTAssertEqual(snapshot.pending, oracle.pending, file: file, line: line)
                    XCTAssertEqual(snapshot.seeking, oracle.seeking, file: file, line: line)
                    XCTAssertEqual(snapshot.waiting, oracle.waiting, file: file, line: line)
                    XCTAssertEqual(snapshot.request, oracle.request, file: file, line: line)
                    XCTAssertEqual(snapshot.lastRequest, oracle.lastRequest, file: file, line: line)
                    XCTAssertEqual(snapshot.correction?.sequence, oracle.correction?.sequence, file: file, line: line)
                    XCTAssertEqual(snapshot.correction?.offset, oracle.correction?.offset, file: file, line: line)
                    XCTAssertEqual(snapshot.correction?.from, oracle.correction?.from, file: file, line: line)
                    XCTAssertEqual(snapshot.correction?.smooth, oracle.correction?.smooth, file: file, line: line)
                    XCTAssertEqual(snapshot.rows.count, oracle.rows.count, file: file, line: line)
                    for (row, expectedRow) in zip(snapshot.rows, oracle.rows) {
                        XCTAssertEqual(row.view, expectedRow.view, file: file, line: line)
                        XCTAssertEqual(row.root, expectedRow.root, file: file, line: line)
                        XCTAssertEqual(row.epoch, expectedRow.epoch, file: file, line: line)
                        XCTAssertEqual(row.index, expectedRow.index, file: file, line: line)
                        XCTAssertEqual(row.size, expectedRow.size, file: file, line: line)
                        XCTAssertEqual(row.measured, expectedRow.measured, file: file, line: line)
                    }
                }
            } else { XCTAssertTrue(NSDictionary(dictionary: a.payload).isEqual(to: expected), file: file, line: line) }
            XCTAssertEqual(a.runs.count, b.runs.count, file: file, line: line)
            for (x, y) in zip(a.runs, b.runs) {
                XCTAssertEqual(x.id, y.id, file: file, line: line)
                XCTAssertEqual(x.parent, y.parent, file: file, line: line)
                XCTAssertEqual(x.props, y.props, file: file, line: line)
                XCTAssertEqual(x.handlers, y.handlers, file: file, line: line)
                XCTAssertEqual(x.paints, y.paints, file: file, line: line)
                XCTAssertEqual(x.hasSchemeColor, y.hasSchemeColor, file: file, line: line)
                XCTAssertEqual(x.range, y.range, file: file, line: line)
                XCTAssertEqual(x.run(dark: false), y.run(dark: false), file: file, line: line)
                XCTAssertEqual(x.run(dark: true), y.run(dark: true), file: file, line: line)
            }
            if a.op == .paragraph { paragraphs += 1 }
        }
        compared += 1; operations += new.ops.count
        return new
    }
    private func convert(_ value: OracleBatchValue) -> BatchValue {
        switch value {
        case .number(let n): return .number(n)
        case .string(let s): return .string(s)
        case .bool(let b): return .bool(b)
        case .array(let a): return .array(a.map(convert))
        case .object(let o): return .object(o.mapValues(convert))
        case .null: return .null
        }
    }
    private func compare(_ text: String) throws -> Batch { try compare(Data(text.utf8)) }

    func testEnvelopeSchedulingAndNativeTimerRecords() throws {
        let fields = #"""
        "ops":[],"timers":true,"motion":true,"canvas":true,"waits":[-2,50,-1,100],
        "waitContinue":true,"frames":true,"canvasOwed":true,"controls":true,"layoutProvisional":true,
        "canvasImages":["map"],"unchanged":true,"storeReadPending":true,"storeActivationPending":true,
        "pending":true,"requests":true,"busy":true,"spatial":true,"clock":-0.0,"timer_due_ms":123.25,
        "landing":true,"stored":true,"error":"problem","modes":{"theme":"dark","ignored":false},
        "live":"applied","unsupported":true,"design":"digest","facts":true,"wakelock":false,
        "mustFinish":false,"alerts":["alert"],"journal":["line"],"announced":["store"],"seq":[3,5],
        "orientationlock":{"types":5,"natural":true,"current":false,"by":[[7,"current"],[8,"natural"]]},
        "nativeTimers":[{"once":true,"tolerance":4294967295,"interval":1,"due":0,"generation":"18446744073709551615","index":4294967295,"future":[{}]}]
        """#
        let batch = try compare("{" + fields + "}")
        XCTAssertEqual(batch.orientation?.types, 5)
        XCTAssertEqual(batch.orientation?.natural, true)
        XCTAssertEqual(batch.orientation?.current, false)
        XCTAssertEqual(batch.orientation?.holders, [#"view 7 "current""#, #"view 8 "natural""#])
        let nulls = try JSONSerialization.jsonObject(with: Data(("{" + fields + "}").utf8)) as! [String: Any]
        let empty = try compare(JSONSerialization.data(withJSONObject: nulls.mapValues { _ in NSNull() }))
        XCTAssertNil(empty.orientation)
        XCTAssertNil(empty.nativeTimers)
        for wire in [#"{"nativeTimers":[],"seq":[],"design":12,"modes":true,"orientationlock":false}"#,
                     #"{"nativeTimers":[],"seq":[1.5,2],"design":false}"#,
                     #"{"seq":[-1,2],"modes":{"ignored":null}}"#] {
            let value = try compare(wire)
            XCTAssertNil(value.seq)
            XCTAssertNil(value.design)
            XCTAssertNil(value.orientation)
        }
        let first = Batch.decode(Data(#"{"timers":null,"timers":true,"nativeTimers":[],"nativeTimers":[false]}"#.utf8))
        XCTAssertNil(first.error)
        XCTAssertFalse(first.timers)
        XCTAssertEqual(first.nativeTimers, [])
        let timer = #""index":0,"generation":"0","due":10,"interval":1,"tolerance":0,"once":false"#
        let last = Batch.decode(Data((#"{"nativeTimers":[{"# + timer + #", "due":20}]}"#).utf8))
        XCTAssertNil(last.error)
        XCTAssertEqual(last.nativeTimers?.first?.due, 20)
        for key in ["index", "generation", "due", "interval", "tolerance", "once"] {
            var row = try JSONSerialization.jsonObject(with: Data(("{" + timer + "}").utf8)) as! [String: Any]
            row.removeValue(forKey: key)
            for value in [row, row.merging([key: NSNull()]) { _, new in new }] {
                let wire = try JSONSerialization.data(withJSONObject: ["ops": [["op": "destroy", "id": 1]], "nativeTimers": [value]])
                let invalid = Batch.decode(wire)
                XCTAssertEqual(invalid.error, "unreadable batch", key)
                XCTAssertTrue(invalid.ops.isEmpty, key)
            }
        }
        for replacement in [#""interval":0"#, #""due":-1"#, #""index":4294967296"#,
                            #""generation":"18446744073709551616""#, #""generation":1"#,
                            #""tolerance":0.5"#, #""once":0"#] {
            let invalid = Batch.decode(Data((#"{"nativeTimers":[{"# + timer + "," + replacement + "}]}").utf8))
            XCTAssertEqual(invalid.error, "unreadable batch", replacement)
        }
    }

    func testInlineColorWireForms() throws {
        let light: [Double] = [1, 2, 3, 4], dark: [Double] = [5, 6, 7, 8]
        let fixtures: [(String, [Double], [Double], Bool, Bool)] = [
            (#"[1,2,3,4]"#, light, light, false, true),
            (#"[[1,2,3,4],[5,6,7,8]]"#, light, dark, true, true),
            (#"{"sys":"exactMissingColor","c":[1,2,3,4]}"#, light, light, true, false),
            (#"{"c":[[1,2,3,4],[5,6,7,8]],"sys":"exactMissingColor"}"#, light, dark, true, false),
            (#"{"sys":"named:ExactMissingAsset","c":[[1,2,3,4],[5,6,7,8]]}"#, light, dark, true, true),
            (#"{"cs":[{"s":"display-p3","v":[1,0,0,1]}],"c":[1,2,3,4]}"#,
             light + [1, 1, 0, 0, 1], light + [1, 1, 0, 0, 1], false, true),
            (#"{"cs":[{"s":"srgb","v":[1,0,0,1]},{"s":"display-p3","v":[0,1,0,1]}],"c":[[1,2,3,4],[5,6,7,8]]}"#,
             light + [0, 1, 0, 0, 1], dark + [1, 0, 1, 0, 1], true, true),
            (#"{"cs":[{"s":"srgb-linear","v":[-0.5,2,0,0.5]}],"c":[1,2,3,4]}"#,
             light + [2, -0.5, 2, 0, 0.5], light + [2, -0.5, 2, 0, 0.5], false, true),
            (#"{"cs":[{"s":"cg:kCGColorSpaceSRGB","i":"relative-colorimetric","v":[0,0,0,1]}]}"#,
             [0, 0, 0, 255, 0, 0, 0, 0, 1], [0, 0, 0, 255, 0, 0, 0, 0, 1], false, true),
            (#"{"cs":[{"s":"cg:kCGColorSpaceGenericGrayGamma2_2","i":"perceptual","v":[0,0.5]}]}"#,
             [0, 0, 0, 128, 0, 0, 0, 0, 0.5], [0, 0, 0, 128, 0, 0, 0, 0, 0.5], false, true)
        ]
        for (color, expectedLight, expectedDark, paired, fixed) in fixtures {
            let batch = try compare(#"{"ops":[{"op":"paragraph","id":1,"runs":[{"id":2,"parent":1,"style":{"text_color":\#(color)}}]}]}"#)
            let run = try XCTUnwrap(batch.ops.first?.runs.first)
            XCTAssertNil(batch.error, color)
            XCTAssertEqual(run.hasSchemeColor, paired, color)
            XCTAssertEqual(run.run(dark: false).color, expectedLight, color)
            XCTAssertEqual(run.run(dark: true).color, expectedDark, color)
            XCTAssertEqual(run.run(dark: false).fixedInk, fixed, color)
            XCTAssertEqual(run.run(dark: true).fixedInk, fixed, color)
        }
        #if os(macOS)
        let name = "textColor"
        #else
        let name = "labelColor"
        #endif
        for name in [name, "@tint", "@tint/0.2"] {
            let batch = try compare(#"{"ops":[{"op":"paragraph","id":1,"runs":[{"id":2,"parent":1,"style":{"text_color":{"sys":"\#(name)","c":[1,2,3,4]}}}]}]}"#)
            let run = try XCTUnwrap(batch.ops.first?.runs.first)
            XCTAssertTrue(run.hasSchemeColor)
            for dark in [false, true] {
                XCTAssertNotEqual(run.run(dark: dark).color, light, name)
                XCTAssertFalse(run.run(dark: dark).fixedInk, name)
            }
        }
    }

    func testInlineVisibilityInEitherOperationFieldOrder() throws {
        for (style, hidden) in [("{}", false), (#"{"visibility":"visible"}"#, false),
                                (#"{"visibility":"hidden"}"#, true), (#"{"visibility":"collapse"}"#, true)] {
            let run = #"{"id":2,"parent":1,"paint":true,"props":{"text":"Run"},"style":\#(style)}"#
            for op in [#"{"op":"paragraph","id":1,"runs":[\#(run)]}"#,
                       #"{"id":1,"runs":[\#(run)],"op":"paragraph"}"#] {
                let batch = Batch.decode(Data(#"{"ops":[\#(op)]}"#.utf8))
                XCTAssertNil(batch.error)
                let decoded = try XCTUnwrap(batch.ops.first?.runs.first)
                XCTAssertEqual(decoded.hidden, hidden, style)
            }
        }
    }

    func testInlineICCColorWireForm() throws {
        let profile = try XCTUnwrap(CGColorSpace(name: CGColorSpace.sRGB)?.copyICCData()) as Data
        let resolver = AssetResolver(root: URL(fileURLWithPath: "/"), names: ["assets/brand.icc"]) { _ in profile }
        for intent in ["relative-colorimetric", "absolute-colorimetric", "perceptual", "saturation"] {
            let data = Data(#"{"ops":[{"op":"paragraph","id":1,"runs":[{"id":2,"parent":1,"style":{"text_color":{"cs":[{"s":"icc:assets/brand.icc","i":"\#(intent)","v":[0,0,0,1]}]}}}]}]}"#.utf8)
            let batch = try compare(data, resolver: resolver)
            let run = try XCTUnwrap(batch.ops.first?.runs.first)
            XCTAssertFalse(run.hasSchemeColor)
            for dark in [false, true] {
                XCTAssertEqual(run.run(dark: dark).color, [0, 0, 0, 255, 0, 0, 0, 0, 1], intent)
                XCTAssertTrue(run.run(dark: dark).fixedInk, intent)
            }
        }
    }

    func testEveryOperationAndAdapterPayload() throws {
        let rows = [
            #"{"op":"create","id":4294967295,"kind":"text","props":{"text":"α 👩‍🚀 é\n\"\\\u0001"},"style":{"opacity":0.5,"text_color":[[1,2,3,4],[5,6,7,8]],"future":{"b":true,"n":null}},"handlers":["press","press","change"]}"#,
            #"{"op":"props","id":1,"set":{"href":"/a"},"clear":["text"]}"#,
            #"{"op":"props","id":0,"set":{"future":"retained","text":""},"clear":["future","text","text"],"future":[true]}"#,
            #"{"op":"props","id":1,"set":null,"clear":null}"#,
            #"{"op":"create","id":1,"kind":null,"props":null,"style":null,"handlers":null}"#,
            #"{"op":"style","id":1,"style":null}"#,
            #"{"op":"style","id":1,"style":{"a":true,"b":false,"c":null,"d":"x","e":[1,"x",{}],"future":{"calc":"user data","env":{"theme":"dark"}}}}"#,
            #"{"op":"children","id":1,"ids":[0,1,4294967295]}"#,
            #"{"op":"roots","ids":[1,2]}"#,
            #"{"op":"paragraph","id":1,"runs":[{"id":2,"parent":1,"paint":true,"props":{"text":"a","href":"/"},"style":{"line_height":1.25,"font_size":17.3,"font_weight":600,"font_family":1,"letter_spacing":0.1,"font_style":"italic","text_decoration_line":"underline","text_color":[[1,2,3,4],[5,6,7,8]],"ignored":[true]},"handlers":["press"]},{"id":3,"parent":2,"style":{"line_height":"21px","text_color":[1,2,3,4]}}]}"#,
            #"{"op":"frame","id":1,"x":-0,"y":0.1,"w":1e3,"h":1.7976931348623157e308}"#,
            #"{"op":"content","id":1,"w":2,"h":3}"#,
            #"{"op":"content","id":1,"w":2,"h":3,"fitHeight":180.5}"#,
            #"{"op":"present","id":1,"property":"translate","x":-2.5,"y":0.125}"#,
            #"{"op":"destroy","id":1,"ignored":{"deep":[null,true,"x"]}}"#,
            #"{"op":"flow","id":1,"shapes":[{"rect":[0,1,2,3]}]}"#,
            #"{"op":"surface","id":1,"name":"map","values":[1,true,null,["x"]]}"#,
            #"{"op":"surface","id":1,"name":"map","values":[{"calc":"user data","env":{"theme":"dark"}}]}"#,
            #"{"op":"command","name":"openURL","args":["https://example.com",false,null]}"#,
            #"{"op":"hold","token":"18446744073709551615","x":1,"y":2}"#,
            #"{"op":"height-drag","id":1,"handleKey":"18446744073709551615","target":null,"targetKey":null}"#,
            #"{"op":"transform-drag","id":1,"runtime":"1","handleKey":"2","target":3,"targetKey":"4","clip":5,"clipKey":"6"}"#,
            #"{"op":"retire-motion","id":1,"property":"translate","runtime":"2","token":"3"}"#,
            #"{"op":"collections","items":[{"id":1,"rows":[]}] }"#,
            #"{"op":"region","id":1,"nested":{"number":0.1,"boolean":true,"null":null}}"#,
            #"{"op":"router","top":2,"url":"/path","removed":[1]}"#,
            #"{"op":"language","lang":"ar","dir":"rtl"}"#,
            #"{"op":"title","title":"A question"}"#,
            #"{"op":"title","title":null}"#,
            #"{"op":"future","id":1,"any":[true,null,{"x":"y"}]}"#
        ]
        for row in rows {
            _ = try compare("{\"ops\":[\(row)],\"timers\":true,\"timer_due_ms\":123.25,\"clock\":-0.0,\"motion\":true,\"pending\":true}")
            // Move op behind all other keys without a JSON round trip changing
            // number spellings. This exercises the reordered path.
            let comma = try XCTUnwrap(row.firstIndex(of: ","))
            let head = row[row.index(after: row.startIndex)..<comma]
            let tail = row[row.index(after: comma)..<row.index(before: row.endIndex)]
            _ = try compare("{\"ops\":[{\(tail),\(head)}]}")
        }
        XCTAssertEqual(compared, rows.count * 2)
    }

    func testReorderedCollectionsRestoreParserDepth() throws {
        for op in [#"{"op":"collections","items":[]}"#,
                   #"{"items":[],"op":"collections"}"#] {
            let ops = Array(repeating: op, count: 512).joined(separator: ",")
            let data = Data("{\"ops\":[\(ops)]}".utf8)
            let batch = try data.withUnsafeBytes {
                var reader = BatchReader(bytes: $0.bindMemory(to: UInt8.self))
                let batch = try reader.batch()
                try reader.end()
                XCTAssertEqual(reader.depth, 0)
                return batch
            }
            try compare(data, decoded: batch)
            XCTAssertNil(batch.error)
            XCTAssertEqual(batch.ops.count, 512)
            XCTAssertTrue(batch.ops.allSatisfy { $0.op == .collections && $0.collections?.isEmpty == true })
        }
    }

    func testCollectionFieldOrderKeepsTheNestingLimit() throws {
        for count in [509, 510] {
            let nested = String(repeating: "[", count: count) + "0" + String(repeating: "]", count: count)
            for op in ["{\"op\":\"collections\",\"items\":[],\"unknown\":\(nested)}",
                       "{\"items\":[],\"unknown\":\(nested),\"op\":\"collections\"}"] {
                let batch = Batch.decode(Data("{\"ops\":[\(op)]}".utf8))
                if count == 509 {
                    XCTAssertNil(batch.error)
                    XCTAssertEqual(batch.ops.count, 1)
                    XCTAssertEqual(batch.ops.first?.collections?.count, 0)
                } else {
                    XCTAssertEqual(batch.error, "unreadable batch")
                    XCTAssertTrue(batch.ops.isEmpty)
                }
            }
        }
    }

    func testStylesThatDifferShareTheValuesTheyRepeat() throws {
        // A colour and a shadow two different styles hold, each decoded once.
        let batch = try compare(#"{"ops":[{"op":"style","id":1,"style":{"border_radius_top_left":4,"background_color":[0.125,0.25,0.5,1],"box_shadow":[{"x":0,"y":1,"c":[0,0,0,0.5]}]}},{"op":"style","id":2,"style":{"border_radius_top_left":8,"background_color":[0.125,0.25,0.5,1],"box_shadow":[{"x":0,"y":1,"c":[0,0,0,0.5]}]}}]}"#)
        func storage(_ value: BatchValue?) throws -> UnsafeRawPointer? {
            try XCTUnwrap(value?.array).withUnsafeBufferPointer { $0.baseAddress.map(UnsafeRawPointer.init) }
        }
        let (a, b) = (batch.ops[0].style, batch.ops[1].style)
        XCTAssertNotEqual(a, b)
        XCTAssertEqual(try storage(a["background_color"]), try storage(b["background_color"]))
        XCTAssertEqual(try storage(a["box_shadow"]), try storage(b["box_shadow"]))
        // A style with an escape is neither cached nor shares: one can spell out a profile path.
        let escapedText = #"{"ops":[{"op":"style","id":3,"style":{"font_family":"A\u0020family","background_color":[0.125,0.25,0.5,1]}}]}"#
        let (escaped, again) = (try compare(escapedText), try compare(escapedText))
        XCTAssertNotEqual(try storage(escaped.ops[0].style["background_color"]), try storage(a["background_color"]))
        XCTAssertNotEqual(try storage(escaped.ops[0].style["background_color"]), try storage(again.ops[0].style["background_color"]))
        // Swift's `==` equates these two spellings; each keeps its own bytes.
        let (composed, decomposed) = ("Caf\u{E9} Display Extra", "Cafe\u{301} Display Extra")
        let spelled = try compare("{\"ops\":[{\"op\":\"style\",\"id\":4,\"style\":{\"font_family\":\"\(composed)\"}},{\"op\":\"style\",\"id\":5,\"style\":{\"font_family\":\"\(decomposed)\",\"x\":1}}]}")
        XCTAssertEqual(spelled.ops.map { Array(($0.style["font_family"]?.string ?? "").utf8) }, [Array(composed.utf8), Array(decomposed.utf8)])
    }

    func testStringsNumbersDefaultsAndUnknownKeys() throws {
        for text in [#"{}"#, #"{"ops":null,"timers":null,"motion":null,"clock":null,"error":null,"pending":null,"timer_due_ms":null}"#,
                     #"{"unknown":{"a":[{},[],true,false,null,1e999]},"ops":[{"op":"destroy","unused":1e999}]}"#,
                     #"{"error":"\"\\\/\b\f\n\r\t\u0000\u007F\u0080\u07FF\u0800\uFFFF\uD83D\uDE80","ops":[]}"#] {
            _ = try compare(text)
        }
        let numbers = ["-0", "0", "-0.0", "1.0000000000000002", "9007199254740993", "18446744073709551615",
                       "5e-324", "2.2250738585072014e-308", "1.7976931348623157e308", "0.84551240822557006", "1e+20", "0e-400"]
        for number in numbers { _ = try compare("{\"clock\":\(number),\"ops\":[{\"op\":\"style\",\"style\":{\"n\":\(number)}}]}") }
        // A nonzero literal below the smallest subnormal is refused by both, as one past the largest is.
        for number in ["1e-324", "-2.4e-324", "1.8e308"] {
            let data = Data("{\"clock\":\(number)}".utf8)
            XCTAssertThrowsError(try JSONDecoder().decode(OracleBatch.self, from: data))
            XCTAssertEqual(Batch.decode(data).error, "unreadable batch")
        }
        // Deterministic finite f64 round trips exercise decimal rounding across
        // the exponent range, with exact bits (including the sign of zero).
        var bits: UInt64 = 1044
        for _ in 0..<2000 {
            bits = bits &* 6364136223846793005 &+ 1442695040888963407
            let value = Double(bitPattern: bits)
            if value.isFinite { _ = try compare("{\"clock\":\(value)}") }
        }
        _ = try compare(#"{"ops":[{"op":"roots","ids":[1.0,1e2,-0,4294967295.0]}]}"#)
    }

    func testMalformedBatchesFailAsOneUnreadableBatch() {
        let malformed = ["", "[]", "null", "{}x", "{", #"{"ops":[{}]}"#,
            #"{"ops":[{"op":"create","id":4294967296}]}"#, #"{"ops":[{"op":"roots","ids":[1.2]}]}"#,
            #"{"ops":[{"op":"create","props":{"text":null}}]}"#, #"{"timers":1}"#,
            #"{"ops":[{"op":"props","set":{"text":1}}]}"#,
            #"{"ops":[{"set":{"text":1},"op":"props"}]}"#,
            #"{"ops":[{"op":"props","clear":[null]}]}"#,
            #"{"ops":[{"clear":[null],"op":"props"}]}"#,
            #"{"ops":[{"op":"style","style":[]}]}"#,
            #"{"ops":[{"style":[],"op":"style"}]}"#,
            #"{"clock":01}"#, #"{"clock":1.}"#, #"{"clock":1e}"#, #"{"clock":NaN}"#, #"{"clock":1e999}"#,
            #"{"error":"\q"}"#, #"{"error":"\uD800"}"#, #"{"error":"\uDC00"}"#, #"{"error":"\uD800\u0041"}"#,
            #"{"ops":[{"op":"paragraph","runs":[{"id":1}]}]}"#,
            #"{"ops":[{"op":"paragraph","runs":[{"id":1,"parent":0,"style":{"text_color":[1,2,3]}}]}]}"#]
        for text in malformed {
            XCTAssertThrowsError(try JSONDecoder().decode(OracleBatch.self, from: Data(text.utf8)), text)
            XCTAssertEqual(Batch.decode(Data(text.utf8)).error, "unreadable batch", text)
            XCTAssertTrue(Batch.decode(Data(text.utf8)).ops.isEmpty, text)
        }
        for color in [#"{}"#, #"{"sys":"exactMissingColor"}"#, #"{"sys":"exactMissingColor","c":[1,2,3]}"#] {
            let data = Data(#"{"ops":[{"op":"paragraph","runs":[{"id":1,"parent":0,"style":{"text_color":\#(color)}}]}]}"#.utf8)
            XCTAssertThrowsError(try JSONDecoder().decode(OracleBatch.self, from: data), color)
            XCTAssertEqual(Batch.decode(data).error, "unreadable batch", color)
            XCTAssertTrue(Batch.decode(data).ops.isEmpty, color)
        }
        for text in [#"{"error":"a"}"#, #"{"ops":[{"op":"paragraph","runs":[{"id":1,"parent":0}]}]}"#] {
            let bytes = Data(text.utf8)
            for end in 0..<bytes.count { XCTAssertEqual(Batch.decode(bytes.prefix(end)).error, "unreadable batch") }
        }
        XCTAssertEqual(Batch.decode(Data([123,34,101,114,114,111,114,34,58,34,0xff,34,125])).error, "unreadable batch")
        XCTAssertEqual(Batch.decode(Data(("{\"unknown\":" + String(repeating: "[", count: 513) + "0" + String(repeating: "]", count: 513) + "}").utf8)).error, "unreadable batch")
    }

    func testRankAdapterKeepsWideIntegersAndZeroInEitherFieldOrder() throws {
        for value in [Int64(-4_294_967_290), 0, 1, 4_294_967_290] {
            for op in ["{\"op\":\"rank\",\"id\":7,\"rank\":\(value)}", "{\"rank\":\(value),\"id\":7,\"op\":\"rank\"}"] {
                let batch = try compare("{\"ops\":[\(op)]}")
                XCTAssertEqual(batch.ops.first?.rank, value)
            }
        }
    }

    func testTypedAdapterDefaultsAndDuplicateFields() throws {
        for row in [#"{"op":"title","title":"A","edited":true}"#,
                    #"{"op":"title","title":null,"edited":null}"#,
                    #"{"op":"title","title":false,"edited":"ignored","future":[1,true]}"#,
                    #"{"op":"rank","rank":null}"#, #"{"op":"rank","rank":"ignored"}"#,
                    #"{"op":"rank","rank":true}"#, #"{"op":"rank","rank":-1.75}"#] {
            _ = try compare("{\"ops\":[\(row)]}")
            var object = try JSONSerialization.jsonObject(with: Data(row.utf8)) as! [String: Any]
            let kind = object.removeValue(forKey: "op") as! String
            let body = String(decoding: try JSONSerialization.data(withJSONObject: object, options: .sortedKeys), as: UTF8.self)
            _ = try compare("{\"ops\":[\(body.dropLast()),\"op\":\"\(kind)\"}]}")
        }
        let duplicate = Batch.decode(Data(#"{"ops":[{"op":"rank","id":1,"id":null,"rank":3,"rank":null},{"op":"title","title":"A","title":null,"edited":true,"edited":null}]}"#.utf8))
        XCTAssertNil(duplicate.error)
        XCTAssertNil(duplicate.ops[0].nodeID)
        XCTAssertNil(duplicate.ops[0].rank)
        XCTAssertNil(duplicate.ops[1].title)
        XCTAssertNil(duplicate.ops[1].edited)
    }

    func testGeometryAdaptersKeepFoundationCastsInEitherFieldOrder() throws {
        let rows = [
            #"{"op":"fieldContent","id":7,"rect":[-1.25,2,30,40],"padding":[null,true,3,"auto"],"cell":1}"#,
            #"{"op":"fieldContent","rect":[true,2,3,4],"padding":[1,2,3],"cell":"ignored"}"#,
            #"{"op":"fieldContent","rect":null,"padding":null,"cell":null}"#,
            #"{"op":"layoutHits","boxes":[[null,[1,2,3,4],5],[true,[false,2,3,4],4294967297,7,"ignored"],[9,[1,2],4],[]]}"#,
            #"{"op":"layoutHits","boxes":[[null,[1,2,3,4],null]]}"#,
            #"{"op":"layoutHits","boxes":[false]}"#,
            #"{"op":"layoutHits","boxes":[]}"#,
            #"{"op":"layoutHitsWithdrawn","ids":[false,-1,4294967297,1.75]}"#,
            #"{"op":"layoutHitsWithdrawn","ids":[1,null]}"#,
            #"{"op":"layoutHitsWithdrawn","ids":[]}"#,
            #"{"op":"layoutPromote","parent":true,"frame":[0,1,2,3],"children":[[4,[1,2,3,4]],[5,[true,2,3,4]],[],[6,[1,2,3,4],7]],"keep":[false,-1]}"#,
            #"{"op":"layoutPromote","parent":"ignored","frame":[0,1,2],"children":[false],"keep":[1,null]}"#
        ]
        for kind in ["fieldContent", "layoutHits", "layoutHitsWithdrawn", "layoutPromote"] {
            _ = try compare("{\"ops\":[{\"op\":\"\(kind)\"}]}")
        }
        for row in rows {
            _ = try compare("{\"ops\":[\(row)]}")
            let comma = try XCTUnwrap(row.firstIndex(of: ","))
            let head = row[row.index(after: row.startIndex)..<comma]
            let tail = row[row.index(after: comma)..<row.index(before: row.endIndex)]
            _ = try compare("{\"ops\":[{\(tail),\(head)}]}")
        }
        let duplicate = Batch.decode(Data(#"{"ops":[{"op":"fieldContent","rect":[1,2,3,4],"rect":null,"padding":[1,2,3,4],"padding":null,"cell":true,"cell":null},{"op":"layoutHits","boxes":[[null,[0,0,1,1],2]],"boxes":null}]}"#.utf8))
        XCTAssertNil(duplicate.error)
        XCTAssertNil(duplicate.ops[0].contentRect)
        XCTAssertNil(duplicate.ops[0].contentPadding)
        XCTAssertNil(duplicate.ops[0].fieldCell)
        XCTAssertNil(duplicate.ops[1].layoutBoxes)
    }

    func testGeneratedInlineStyleDispatchAndDeferredHeight() throws {
        let style = #"{"line_height":1.5,"font_size":20,"font_weight":650,"font_family":2,"font_stretch":1.25,"font_style":"italic","letter_spacing":0.5,"font_variant_numeric":3,"font_variant_ligatures":2,"font_optical_sizing":"none","font_variation_settings":[],"font_feature_settings":[],"visibility":"hidden","user_select":"all","outline_style":"none","text_decoration_line":"underline","text_color":[1,2,3,4],"background_color":[[5,6,7,8],[9,10,11,12]],"border_color_left":[1,2,3,4],"border_style_right":"dashed","border_radius_bottom_right":5,"text_shadow":{"o":[1,2],"b":3},"text_stroke_width":2,"text_stroke_color":[3,4,5,6],"future":{"ignored":true}}"#
        for op in ["{\"op\":\"paragraph\",\"runs\":[{\"id\":2,\"parent\":1,\"opacity\":0.4,\"style\":\(style)}]}",
                   "{\"runs\":[{\"style\":\(style),\"opacity\":0.4,\"parent\":1,\"id\":2}],\"op\":\"paragraph\"}"] {
            let batch = Batch.decode(Data("{\"ops\":[\(op)]}".utf8))
            XCTAssertNil(batch.error)
            let row = try XCTUnwrap(batch.ops.first?.runs.first)
            let run = row.lightRun
            XCTAssertEqual(run.size, 20); XCTAssertEqual(run.lineHeight, 30)
            XCTAssertEqual(run.weight, 650); XCTAssertEqual(run.family, 2)
            XCTAssertEqual(run.stretch, 1.25); XCTAssertEqual(run.letterSpacing, 0.5)
            XCTAssertEqual(run.numeric, 3); XCTAssertEqual(run.ligatures, 2)
            XCTAssertTrue(run.italic); XCTAssertTrue(run.hidden)
            XCTAssertTrue(row.selectsAll); XCTAssertTrue(row.outlineNone)
            XCTAssertEqual(row.opacity, 0.4)
            XCTAssertEqual(run.decoration, "underline"); XCTAssertTrue(row.fixedInk)
            XCTAssertEqual(row.darkBackground, [9,10,11,12])
            XCTAssertEqual(row.borderColors[3], [1,2,3,4])
            XCTAssertEqual(row.paint.shadow, [1,2,3]); XCTAssertEqual(row.paint.strokeWidth, 2)
            XCTAssertEqual(row.paint.strokeColor, [3,4,5,6])
        }
    }

    func testTransformReplyUsesTheSameBatchReader() throws {
        let wire = #"{"accepted":true,"committed":true,"runtime":"18446744073709551615","geometrySequence":"2","translateToken":"3","scaleToken":"4","value":[1,2,3],"batch":{"ops":[{"op":"present","id":1,"x":0.1}],"timer_due_ms":123.25,"timers":true}}"#
        let reply = try XCTUnwrap(TransformDragReply(Data(wire.utf8)))
        XCTAssertTrue(reply.accepted); XCTAssertTrue(reply.committed)
        XCTAssertEqual(reply.runtime, UInt64.max); XCTAssertEqual(reply.sequence, 2)
        XCTAssertEqual(reply.translate, 3); XCTAssertEqual(reply.scale, 4)
        XCTAssertEqual(reply.value?.values, [1,2,3]); XCTAssertEqual(reply.batch.timerDueMs, 123.25)
        XCTAssertNil(TransformDragReply(Data(#"{"batch":{"ops":[{}]}}"#.utf8)))
    }
}

#if os(macOS)
extension BatchDecodeTests {
    func testEveryBatchFromRealSession() throws {
        _ = NSApplication.shared
        let session = ExactApp.shared.makeSession(label: "batch-equivalence")
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 900, height: 700),
                              styleMask: [], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = session.presenter.viewport
        session.clock = 0
        session.runtime.observeBatch = { [unowned self] data, batch in
            do { try self.compare(data, decoded: batch, resolver: ExactApp.shared.resolver) }
            catch { XCTFail("Foundation refused real session batch: \(error)") }
        }
        defer { session.runtime.observeBatch = nil; session.destroy(); window.close() }
        XCTAssertNil(session.boot(size: CGSize(width: 900, height: 700)).error)
        func drain() { RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.002)) }
        func node(_ id: String) -> NodeView? { session.presenter.views.values.first { $0.props["testId"] == id } }
        if node("markdown-main") != nil { // the Markdown viewer's root (the LLP reader also has an `open-file`)
            // The command running this test can name the actual probe corpus
            // and dense document; normal host tests use repository documents.
            let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
                .deletingLastPathComponent().deletingLastPathComponent()
                .deletingLastPathComponent().deletingLastPathComponent()
            let paths = ProcessInfo.processInfo.environment["EXACT_BATCH_DOCUMENTS"]?.components(separatedBy: "\n")
                ?? [root.appendingPathComponent("README.md").path,
                    root.appendingPathComponent("docs/contract-for-agents.md").path]
            for path in paths {
                let before = compared, beforeOps = operations, beforeParagraphs = paragraphs
                XCTAssertTrue(FileManager.default.fileExists(atPath: path), path)
                XCTAssertTrue(session.change(testId: "open-file", value: path))
                let deadline = Date(timeIntervalSinceNow: 30)
                let name = URL(fileURLWithPath: path).lastPathComponent
                while node("document-name")?.props["text"] != name && Date() < deadline {
                    session.apply(session.runtime.pump(now: session.now())); drain()
                }
                XCTAssertEqual(node("document-name")?.props["text"], name)
                let list = try XCTUnwrap(node("document"))
                let scroll = try XCTUnwrap(list.scroll)
                for fraction in (0...128).map({ CGFloat($0) / 128 }) + [0.5, 0.1, 0.9, 0] {
                    let extent = max(0, (scroll.documentView?.frame.height ?? 0) - scroll.contentSize.height)
                    scroll.contentView.scroll(to: CGPoint(x: 0, y: extent * fraction))
                    session.presenter.scrolled()
                    session.presenter.pump()
                    drain()
                }
                for width: CGFloat in [640, 1200, 900] {
                    session.resize(CGSize(width: width, height: 700)); drain()
                }
                XCTAssertGreaterThan(paragraphs - beforeParagraphs, 0)
                print("BATCH EQUIVALENCE \(name): batches=\(compared - before) ops=\(operations - beforeOps) paragraphs=\(paragraphs - beforeParagraphs)")
            }
        } else {
            try caltrainArchive()
            let change = try XCTUnwrap(node("change-station"))
            session.apply(session.runtime.press(change.id, now: 0))
            // The search hears each keystroke (`input`); it has no `change`.
            let search = try XCTUnwrap(node("station-search"))
            let typed = session.runtime.input(search.id, "Palo", now: 0)
            XCTAssertNil(typed.error)
            session.apply(typed)
            for _ in 0..<8 { session.apply(session.runtime.pump(now: 0)); drain() }
            session.apply(session.runtime.advance(now: 60_000))
            for width: CGFloat in [390, 900] { session.resize(CGSize(width: width, height: 700)); drain() }
            print("BATCH EQUIVALENCE Caltrain: batches=\(compared) ops=\(operations) paragraphs=\(paragraphs)")
        }
        XCTAssertGreaterThan(compared, 4)
        XCTAssertGreaterThan(operations, 20)
    }
}
#endif
