import ExactCore
import CExact
import Foundation

extension Runtime {
    package func collectionFixedSizes(_ view: UInt32, lead: Int, trail: Int) -> [Double]? {
        guard let fixedSize = CoreExports.collectionFixedSize else { return nil }
        return on(busy: nil) {
            let sizes = (0..<2).map { fixedSize(rt, view, UInt32(lead), UInt32(trail), UInt32($0)) }
            return sizes.allSatisfy { $0.isFinite && $0 >= 0 } ? sizes : nil
        }
    }

    func collectionSnap(_ view: UInt32, proposed: Double, port: Double, minimum: Double = 0, maximum: Double, distance: Double = .infinity) -> Double? {
        guard let snap = CoreExports.collectionSnap else { return nil }
        return on(busy: nil) {
            let value = snap(rt, view, proposed, port, minimum, maximum, distance)
            return value.isFinite ? value : nil
        }
    }

    func collectionSnapStop(_ view: UInt32, release: Double, rest: Double, port: Double, minimum: Double, maximum: Double) -> Double? {
        guard let snapStop = CoreExports.collectionSnapStop else { return nil }
        return on(busy: nil) {
            let value = snapStop(rt, view, release, rest, port, minimum, maximum)
            return value.isFinite ? value : nil
        }
    }

    convenience init() {
        self.init(hooks: RuntimeHooks(
            decodeBatch: { Batch.decode($0, resolver: $1) },
            prepareBatch: { runtime, batch in
                batch.prepare()
                batch.takeBeneath(runtime)
                #if os(macOS)
                batch.takeControls(runtime)
                #elseif os(iOS)
                batch.takeSplit(runtime)
                #endif
            }, symbolMeasure: SymbolMeasure.measure,
            prewarmIslands: SvgLink.prewarm))
    }

    func holdBegin(_ view: UInt32, property: UInt32, now: Double) -> (NativeHold?, Batch) {
        if enter() { return (nil, Runtime.heldBatch) }
        return on(busy: (nil, Runtime.busy)) {
            let batch = read(exact_hold_begin(rt, view, property, now))
            let start = batch.ops.first { $0.op == .hold }.flatMap { NativeHold($0.payload) }
            return (start, batch)
        }
    }
    package func heightDragBegin(_ handleKey: UInt64, targetKey: UInt64, now: Double) -> (NativeHold?, Batch) {
        if enter() { return (nil, Runtime.heldBatch) }
        return on(busy: (nil, Runtime.busy)) {
            let batch = read(exact_height_drag_begin(rt, handleKey, targetKey, now))
            return (batch.ops.first { $0.op == .hold }.flatMap { NativeHold($0.payload) }, batch)
        }
    }
    /// `change` at `view` with no field selection, for a family outside the core.
    package func valueChange(_ view: UInt32, _ value: String, now: Double) -> Batch { change(view, value, now: now) }
    /// A `change`; a text field's carries its selection as the edit left
    /// it (kind 41, x2apps codeedit #2), else the value alone (kind 1).
    func change(_ view: UInt32, _ value: String, selection: FieldSelection? = nil, now: Double) -> Batch {
        return on {
            let n = write(selection?.payload(value) ?? value)
            return read(exact_dispatch(rt, view, selection == nil ? 1 : 41, n, now))
        }
    }
    /// A text field's value as it moves: HTML's `input` (LLP 1069.001 D4),
    /// with its selection when the host has one (kind 40, else 23).
    func input(_ view: UInt32, _ value: String, selection: FieldSelection? = nil, now: Double) -> Batch {
        return on {
            let n = write(selection?.payload(value) ?? value)
            return read(exact_dispatch(rt, view, selection == nil ? 23 : 40, n, now))
        }
    }
    /// The scripted replacement's one input, with its DOM inputType (kind 48).
    func replacementInput(_ view: UInt32, _ value: String, _ selection: FieldSelection, now: Double) -> Batch {
        on { read(exact_dispatch(rt, view, 48, write(selection.payload(value)), now)) }
    }
    /// A text field's `select` (kind 42, x2apps codeedit #2): its value and
    /// the selection it now has.
    func fieldSelect(_ view: UInt32, _ value: String, _ selection: FieldSelection, now: Double) -> Batch {
        on { read(exact_dispatch(rt, view, 42, write(selection.payload(value)), now)) }
    }
    /// The wall clock owns time, or the agent's clock does (the timers RFC, D1).
    func liveClock(_ yes: Bool) { setLiveClock(yes ? { ExactEnv.wall() } : nil) }
    /// The posture and the viewport segments (LLP 1078 D4): the rects as `x y w h` floats in the input buffer, none for one segment.
    func segments(_ fold: ViewportFold) -> Batch {
        on(layout: true) {
            var data = Data(capacity: fold.rects.count * 16)
            for r in fold.rects { for v in [Float(r.minX), Float(r.minY), Float(r.width), Float(r.height)] { withUnsafeBytes(of: v.bitPattern.littleEndian) { data.append(contentsOf: $0) } } }
            _ = write(data)
            return read(exact_segments(rt, fold.posture == "folded" ? 1 : 0, UInt32(fold.cols), UInt32(fold.rows), UInt32(fold.rects.count)))
        }
    }
    /// @ref LLP 1095 D1 — every colour reference the kernel resolves itself
    /// (paint motion, gradients, SVG scenes), resolved by the platform in
    /// both appearances under the current contrast and reported, so none of
    /// them is a frozen fallback. A name the platform lacks is left out: the
    /// kernel keeps its fallback pair. `tint` is the app's tint,
    /// light then dark (`SystemColor.tintPair`), read on main: the owner
    /// thread has no window to read it from; without one `@tint` is not
    /// reported, and the kernel keeps what it had.
    func reportColors(tint: [[Double]]?) -> Batch {
        on(layout: true) {
            let len = exact_color_references(rt)
            let json = Data(bytes: exact_out(rt), count: Int(len))
            let refs = (try? JSONSerialization.jsonObject(with: json)) as? [[Any]] ?? []
            var bytes = Data(capacity: refs.count * 16)
            for r in refs {
                guard r.count == 3, let kind = r[0] as? Int, let id = r[1] as? Int, let name = r[2] as? String,
                      tint != nil || !name.hasPrefix("@tint") else { continue }
                for dark in [false, true] {
                    guard let c = SystemColor.channels(name, dark: dark, tint: tint?[dark ? 1 : 0], fallback: nil), c.count == 4 else { continue }
                    bytes.append(UInt8(kind)); bytes.append(dark ? 1 : 0)
                    withUnsafeBytes(of: UInt16(id).littleEndian) { bytes.append(contentsOf: $0) }
                    for v in c { bytes.append(UInt8(max(0, min(255, v.rounded())))) }
                }
            }
            return read(exact_colors(rt, write(bytes)))
        }
    }
    /// A button's face, custom or native (LLP 1069.011.000 D1).
    func buttonFace(_ view: UInt32) -> ButtonFace {
        return on(busy: ButtonFace()) {
            let len = exact_press_face(rt, view)
            return ButtonFace(json: Data(bytes: exact_out(rt), count: Int(len)))
        }
    }
    /// A select's options and the one it shows (LLP 1069.001 D5).
    func selectOptions(_ view: UInt32) -> SelectMenu {
        return on(busy: SelectMenu(json: Data())) {
            let len = exact_select_options(rt, view)
            return SelectMenu(json: Data(bytes: exact_out(rt), count: Int(len)))
        }
    }
    /// A radio's group and the radios its arrows move to (x2apps survey #2).
    func radioGroup(_ view: UInt32) -> RadioGroup {
        return on(busy: RadioGroup(json: Data())) {
            let len = exact_radio_group(rt, view)
            return RadioGroup(json: Data(bytes: exact_out(rt), count: Int(len)))
        }
    }
    /// A painted group's keys and Tab stop for member `view` (LLP 1166.000
    /// D2), with `key` pressed and the focus at `focused`; nil for no member.
    func roleKeys(_ view: UInt32, key: String?, focused: UInt32?) -> RoleKeys? {
        return on(busy: nil) {
            let len = exact_role_keys(rt, view, focused ?? 0, write(key ?? ""))
            return RoleKeys(json: Data(bytes: exact_out(rt), count: Int(len)))
        }
    }
    /// What native containers cover of boxes (LLP 1075.003 §3.5), under one
    /// layout: `edges` add to a box's padding; `whole` lays it out as
    /// `display: none`, and `bar` too but for the blocks given inside it;
    /// nil clears.
    package func covers(_ covers: [(UInt32, HostCover?)]) -> Batch {
        return on(layout: true) {
            var bytes = Data(capacity: covers.count * 24)
            for (view, cover) in covers {
                let (kind, edges): (UInt32, [CGFloat]) = switch cover {
                case .none: (0, [0, 0, 0, 0])
                case .edges(let e)?: (1, [e.top, e.right, e.bottom, e.left])
                case .whole?: (2, [0, 0, 0, 0])
                case .bar?: (26, [0, 0, 0, 0])
                case .inset(let top, let bottom)?: (3, [top, 0, bottom, 0])
                case .gutter(let right, let bottom)?: (12, [0, right, bottom, 0])
                case .rowSpacing(let top, let bottom)?: (25, [top, 0, bottom, 0])
                }
                for word in [view, kind] + edges.map({ Float($0).bitPattern }) {
                    withUnsafeBytes(of: word.littleEndian) { bytes.append(contentsOf: $0) }
                }
            }
            return read(exact_host_covers(rt, write(bytes)))
        }
    }
    /// Content owners' local insets (LLP 1123 D9), top right bottom left, under
    /// one layout: the owner by its root's view, nil for the session root's;
    /// nil insets take them back.
    func localInsets(_ owners: [(UInt32?, [CGFloat]?)]) -> Batch {
        return on(layout: true) {
            var bytes = Data(capacity: owners.count * 24)
            for (view, insets) in owners {
                let kind: UInt32 = (view == nil ? 19 : 17) + (insets == nil ? 1 : 0)
                for word in [view ?? 0, kind] + (insets ?? [0, 0, 0, 0]).map({ Float($0).bitPattern }) {
                    withUnsafeBytes(of: word.littleEndian) { bytes.append(contentsOf: $0) }
                }
            }
            return read(exact_host_covers(rt, write(bytes)))
        }
    }
    /// Content owners' updates (LLP 1123 D2), each the owner by its root's view and its
    /// update in the kernel's encoding (`OwnerUpdateIOS.encode`), and targets native code
    /// registered (D9, `ExactNativeTarget.encode`), under one layout, checked whole:
    /// `exact_host_covers` kinds 21 and 22, each record's length after its kind.
    func ownerReports(updates: [(UInt32, Data)] = [], registrations: [Data] = []) -> Batch {
        return on(layout: true) {
            var bytes = Data()
            let records = updates.map { ($0.0, UInt32(21), $0.1) } + registrations.map { (UInt32(0), UInt32(22), $0) }
            for (view, kind, record) in records {
                for word in [view, kind, UInt32(record.count)] {
                    withUnsafeBytes(of: word.littleEndian) { bytes.append(contentsOf: $0) }
                }
                bytes.append(record)
            }
            return read(exact_host_covers(rt, write(bytes)))
        }
    }
    /// The border box a person dragged `view`'s `resize` grip to, under one
    /// layout (`Runner::resize_box`); nil keeps an axis.
    func resizeBox(_ view: UInt32, width: CGFloat?, height: CGFloat?) -> Batch {
        return on {
            var bytes = Data(capacity: 24)
            let lengths = [width, height, 0, 0].map { Float($0 ?? .nan).bitPattern }
            for word in [view, 16] + lengths {
                withUnsafeBytes(of: word.littleEndian) { bytes.append(contentsOf: $0) }
            }
            return read(exact_host_covers(rt, write(bytes)))
        }
    }
    #if os(macOS)
    /// The tracks an AppKit resize changed, each by its header. A withdrawn
    /// width returns the column to CSS automatic sizing.
    func tableColumnWidths(_ columns: [(UInt32, CGFloat?)]) -> Batch {
        on(layout: true) {
            var bytes = Data(capacity: columns.count * 24)
            for (header, width) in columns {
                for word: UInt32 in [header, 24, Float(width ?? .nan).bitPattern, 0, 0, 0] {
                    withUnsafeBytes(of: word.littleEndian) { bytes.append(contentsOf: $0) }
                }
            }
            return read(exact_host_covers(rt, write(bytes)))
        }
    }
    #endif
    /// The margins UIKit's own list gives each grouped list's cards and
    /// cells, under one layout: the card's left and right insets, then the
    /// cell's left and right margins; nil clears them.
    package func groupedMargins(_ lists: [(UInt32, [CGFloat]?)]) -> Batch {
        return on(layout: true) {
            var bytes = Data(capacity: lists.count * 24)
            for (view, margins) in lists {
                let lengths = (margins ?? [0, 0, 0, 0]).map { Float($0).bitPattern }
                for word in [view, margins == nil ? 9 : 8] + lengths {
                    withUnsafeBytes(of: word.littleEndian) { bytes.append(contentsOf: $0) }
                }
            }
            return read(exact_host_covers(rt, write(bytes)))
        }
    }
    /// The margin UIKit's cell gives each cells list's rows at its left and
    /// right, under one layout (LLP 1138); nil clears it.
    package func listMargins(_ lists: [(UInt32, [CGFloat]?)]) -> Batch {
        return on(layout: true) {
            var bytes = Data(capacity: lists.count * 24)
            for (view, margins) in lists {
                let lengths = ((margins ?? [0, 0]) + [0, 0]).map { Float($0).bitPattern }
                for word in [view, margins == nil ? 11 : 10] + lengths {
                    withUnsafeBytes(of: word.littleEndian) { bytes.append(contentsOf: $0) }
                }
            }
            return read(exact_host_covers(rt, write(bytes)))
        }
    }
}

extension Batch {
    /// On the owner as the batch is read: where Back goes from the router's
    /// new top (`ExactSession.locationBeneath`).
    mutating func takeBeneath(_ runtime: Runtime) {
        guard let op = ops.last(where: { $0.op == .router }), let top = (op.payload["top"] as? NSNumber)?.uint64Value else { return }
        let length = exact_location_beneath(runtime.rt, top)
        beneath = (top, String(decoding: Data(bytes: exact_out(runtime.rt), count: Int(length)), as: UTF8.self))
    }
}

#if os(iOS)
extension Batch {
    /// The ops that can change what the split rule reads: the tree, a node's
    /// props, style or frame (a list's direction reaches the host only as its
    /// rows' frames), and the router.
    static let movesSplit: Set<BatchOp.Kind> = [.create, .destroy, .children, .roots, .renew, .reorder, .props, .style, .frame, .router]
    /// On the owner as the batch is read, with the runtime as it made it: the
    /// split rule of the stack main last asked about, when the batch can
    /// change it. Main keeps the last it applied (`NavigationHost.splitRule`),
    /// so it never waits on the owner to learn it (LLP 1072 T2).
    mutating func takeSplit(_ runtime: Runtime) {
        guard let asked = runtime.splitAsked, ops.contains(where: { Self.movesSplit.contains($0.op) }) else { return }
        split = runtime.readSplit(asked.view, retained: asked.retained, sequence: sequence)
    }
}

extension Runtime {
    /// On the owner: the split rule of the stack whose root route is `view`,
    /// which each later batch that can change it carries.
    package func readSplit(_ view: UInt32, retained: UInt32, sequence: UInt64) -> SplitRead {
        splitAsked = (view, retained)
        let length = exact_stack_split(rt, view, retained, 0)
        return SplitRead(view: view, retained: retained, json: Data(bytes: exact_out(rt), count: Int(length)), sequence: sequence)
    }
}
#endif

#if os(macOS)
extension Batch {
    /// The ops after which a native control's state may differ.
    static let dropsFace: Set<BatchOp.Kind> = [.create, .renew, .props, .style, .children, .paragraph, .flow, .frame, .fieldContent]
    /// On the owner as the batch is read, with the runtime as it made it, the
    /// state of the native controls it may change that differs from what the
    /// last batch read carried: the face of each button and the options of
    /// each select it makes or has such an op on (every one when a control's
    /// viewless contents changed, `controls`), and every radio's group when it
    /// changes the tree or a control.
    mutating func takeControls(_ runtime: Runtime) {
        var asked = Set<UInt32>(), regroup = controls
        for op in ops {
            if op.op == .create, op.kind == "control" {
                runtime.controlTypes[op.id] = op.props["type"] ?? ""; runtime.controlsRead[op.id] = nil; asked.insert(op.id)
            } else if op.op == .destroy {
                runtime.controlTypes[op.id] = nil; runtime.controlsRead[op.id] = nil
            } else if Self.dropsFace.contains(op.op), runtime.controlTypes[op.id] != nil {
                if let type = op.props["type"] { runtime.controlTypes[op.id] = type } else if op.clear.contains("type") { runtime.controlTypes[op.id] = "" }
                asked.insert(op.id)
            }
            if BatchFootprint.structuralOps.contains(op.op) { regroup = true }
        }
        if controls { asked.formUnion(runtime.controlTypes.keys) }
        if regroup || !asked.isEmpty { asked.formUnion(runtime.controlTypes.filter { $0.value == "radio" }.keys) }
        func json(_ length: UInt32) -> Data { Data(bytes: exact_out(runtime.rt), count: Int(length)) }
        var reads: [UInt32: ControlRead] = [:]
        for id in asked {
            guard let type = runtime.controlTypes[id] else { continue }
            let last = runtime.controlsRead[id] ?? ControlRead()
            var now = ControlRead()
            if type == "button" { now.face = json(exact_press_face(runtime.rt, id)) }
            if type == "select" { now.select = json(exact_select_options(runtime.rt, id)) }
            if type == "radio" { now.radio = json(exact_radio_group(runtime.rt, id)) }
            runtime.controlsRead[id] = now
            let changed = ControlRead(face: now.face == last.face ? nil : now.face, select: now.select == last.select ? nil : now.select,
                                      radio: now.radio == last.radio ? nil : now.radio)
            if changed != ControlRead() { reads[id] = changed }
        }
        controlReads = reads
    }
}
#endif
