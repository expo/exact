// The dev loop's file form (LLP 1007 §6): a local development bake, a plan
// with its optional module pair, applied to every session when it changes,
// or installed as the launch generation. The dev menus' Reload applies it;
// the network form is the plan from a URL (PlanURL.swift).
import Foundation

/// A fixed JavaScript/Rust generation crosses the data seam as one candidate.
package enum DevelopmentModules {
    package static func combine(javascript: ExactModule?, rust: ExactModule?) throws -> ExactModule? {
        guard let javascript else { return rust }
        guard let rust else { return javascript }
        guard let jsReceipt = try JSONSerialization.jsonObject(with: javascript.receipt) as? [String: Any],
              let rustReceipt = try JSONSerialization.jsonObject(with: rust.receipt) as? [String: Any] else {
            throw CocoaError(.fileReadCorruptFile)
        }
        let receipt = try JSONSerialization.data(withJSONObject: [
            "version": 1, "kind": "mixed", "javascript": jsReceipt, "rust": rustReceipt,
            "javascriptBytes": javascript.bytecode.count,
        ], options: .sortedKeys)
        guard receipt.count <= 1 << 20, javascript.bytecode.count <= 32 << 20,
              rust.bytecode.count <= (32 << 20) - javascript.bytecode.count else { throw CocoaError(.fileReadTooLarge) }
        var bytes = javascript.bytecode
        bytes.append(rust.bytecode)
        return ExactModule(receipt: receipt, bytecode: bytes)
    }
}

/// A local development bake is a plan plus its optional, inseparable module
/// pair (LLP 1027 D7). Watch all three files: a writer can replace them in
/// either order, and a refused partial bake must be retried when it finishes.
public struct ExactDevelopmentPlan {
    private let baseFiles: [URL]
    private var files: [URL] {
        let root = baseFiles[0].deletingLastPathComponent().appendingPathComponent("rust")
        let found = (FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil)?.allObjects as? [URL] ?? [])
            .filter { !$0.hasDirectoryPath }.sorted { $0.path < $1.path }
        return baseFiles + found
    }
    public init(_ path: String) {
        let plan = URL(fileURLWithPath: path)
        let directory = plan.deletingLastPathComponent()
        baseFiles = [plan, directory.appendingPathComponent("app.module.json"), directory.appendingPathComponent("app.hbc")]
    }

    public var hasModule: Bool { files.dropFirst().contains { FileManager.default.fileExists(atPath: $0.path) } }

    /// Identity and subsecond modification time, including missing files.
    public var revision: [Double] {
        files.flatMap { url -> [Double] in
            let a = try? FileManager.default.attributesOfItem(atPath: url.path)
            return [(a?[.modificationDate] as? Date)?.timeIntervalSinceReferenceDate ?? -1,
                    (a?[.size] as? NSNumber)?.doubleValue ?? -1,
                    (a?[.systemFileNumber] as? NSNumber)?.doubleValue ?? -1]
        }
    }

    /// The plan and, when one is beside it, its module pair, read as one
    /// candidate: a partial pair or an oversized file is refused.
    private func read(for app: ExactApp) throws -> (plan: Data, module: ExactModule?) {
        let files = self.files
        func read(_ index: Int, limit: Int) throws -> Data {
            let url = files[index]
            let a = try FileManager.default.attributesOfItem(atPath: url.path)
            guard let size = a[.size] as? NSNumber, size.intValue <= limit else {
                throw NSError(domain: "ExactDevelopmentPlan", code: 1, userInfo: [NSLocalizedDescriptionKey: "\(url.lastPathComponent) exceeds the development artifact limit"])
            }
            let bytes = try Data(contentsOf: url)
            guard bytes.count <= limit else { throw CocoaError(.fileReadTooLarge) }
            return bytes
        }
        let plan = try read(0, limit: 32 << 20)
        guard hasModule else { return (plan, nil) }
        var javascript: ExactModule?, rust: ExactModule?
        if files[1...2].contains(where: { FileManager.default.fileExists(atPath: $0.path) }) {
            javascript = ExactModule(receipt: try read(1, limit: 1 << 20), bytecode: try read(2, limit: 32 << 20))
        }
        if files.count > baseFiles.count && !(app.rustPolicy.mode == "off" && javascript != nil) {
            let policy = app.rustPolicy
            let prefix = policy.mode == "wasm" ? "rust/wasm" : "rust/" + policy.mode + "/" + policy.target
            guard policy.mode != "off",
                  let index = files.firstIndex(where: { $0.path.hasSuffix(prefix + "/app.module.json") }) else {
                throw NSError(domain: "ExactDevelopmentPlan", code: 1, userInfo: [NSLocalizedDescriptionKey: "no Rust replacement matching this binary's executor and target"])
            }
            let receiptIndex = index
            let json = try JSONSerialization.jsonObject(with: read(index, limit: 1 << 20)) as? [String: Any]
            guard let row = json?["module"] as? [String: Any], let name = row["file"] as? String,
                  ["app.module.wasm", "app.module.dylib", "app.module.so", "app.module.dll", "app.module.bin"].contains(name),
                  let index = files.firstIndex(where: { $0.path.hasSuffix(prefix + "/" + name) }) else { throw CocoaError(.fileNoSuchFile) }
            rust = ExactModule(receipt: try read(receiptIndex, limit: 1 << 20), bytecode: try read(index, limit: 32 << 20))
        }
        guard let module = try DevelopmentModules.combine(javascript: javascript, rust: rust) else { throw CocoaError(.fileNoSuchFile) }
        return (plan, module)
    }

    @discardableResult
    public func apply(to app: ExactApp) -> ExactApplyResult { apply(to: app, now: false) }

    /// `now`: the dev menu's Reload, which no interaction holds (LLP 1029.000.002 D9).
    func apply(to app: ExactApp, now: Bool) -> ExactApplyResult {
        do {
            let (plan, module) = try read(for: app)
            let label = baseFiles[0].lastPathComponent
            return app.applyTogether(plan, label: label, resolver: app.resolver, token: 0, module: module, development: true, now: now, commit: { true })
        } catch {
            fputs("exact: local generation refused: \(error.localizedDescription)\n", stderr)
            return .refused(error.localizedDescription)
        }
    }

    /// The plan and its module pair as the launch generation (`EXACT_PLAN`
    /// naming a local bake, LLP 1116.003): the first boot runs it as a build
    /// that baked it would, fresh, with nothing carried from the embedded
    /// plan. Call before the first session; nil once installed, else why not.
    public func installInitial(on app: ExactApp) -> String? {
        do {
            let (plan, module) = try read(for: app)
            guard let module else { return "\(baseFiles[0].lastPathComponent) has no module beside it" }
            app.installInitial(ExactGeneration(plan: plan, assets: app.resolver, module: module))
            return nil
        } catch {
            return "local generation refused: \(error.localizedDescription)"
        }
    }

    /// The dev loop's file form (LLP 1007 §6): every 100 ms, a changed
    /// revision is applied to `app`. A module pair starts unseen, so the
    /// first tick applies it. Keep the timer for as long as the watch runs.
    public func watch(_ app: ExactApp) -> DispatchSourceTimer {
        var last = hasModule ? [] : revision
        let t = DispatchSource.makeTimerSource(queue: .main)
        t.schedule(deadline: .now() + 0.1, repeating: 0.1)
        t.setEventHandler {
            last = Self.seen(revision, last: last, apply: { apply(to: app) })
        }
        t.resume()
        return t
    }

    /// The revision seen after a tick: each is applied once, whatever it
    /// answers. One that must wait is the app's to apply (LLP 1029.000.002 D9).
    static func seen(_ revision: [Double], last: [Double], apply: () -> ExactApplyResult) -> [Double] {
        guard revision != last else { return last }
        _ = apply()
        return revision
    }
}
