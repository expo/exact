// A package's implementation loads at its first use (LLP 1067.000.001).
import Foundation
import CoreGraphics
import Security

#if canImport(ExactPackage)
import ExactPackage
#if os(iOS) || os(tvOS)
import UIKit
#endif

private let packageLibraryAnchor: @convention(c) () -> Void = {}

enum ExactPackageLibrary {
    typealias Create = @convention(c) (UnsafeMutableRawPointer) -> UnsafeMutableRawPointer
    private static var entries: [String: Result<Create, ExactNativeRefusal>] = [:]
    static var loadSeconds = 0.0

    private static func path(_ name: String) -> String {
        // The seam and the package are siblings, including in an embedded host.
        var info = Dl_info()
        let symbol = unsafeBitCast(packageLibraryAnchor, to: UnsafeRawPointer.self)
        guard dladdr(symbol, &info) != 0, let file = info.dli_fname else { return "lib\(name).dylib" }
        let parent = URL(fileURLWithPath: String(cString: file)).deletingLastPathComponent()
        if parent.pathExtension == "framework" {
            return parent.deletingLastPathComponent().appendingPathComponent("\(name).framework/\(name)").path
        }
        return parent.appendingPathComponent("lib\(name).dylib").path
    }

    static func loaded(_ name: String) -> Bool {
        dlopen(path(name), RTLD_NOW | RTLD_NOLOAD) != nil
    }

    private static func entry(_ name: String) -> Result<Create, ExactNativeRefusal> {
        if let entry = entries[name] { return entry }
        let started = ContinuousClock.now
        defer {
            let elapsed = started.duration(to: .now).components
            loadSeconds += Double(elapsed.seconds) + Double(elapsed.attoseconds) / 1e18
        }
        let file = path(name)
        let result: Result<Create, ExactNativeRefusal>
        if let library = dlopen(file, RTLD_NOW | RTLD_LOCAL | RTLD_FIRST) {
            if let create = dlsym(library, "exact_package_create") {
                result = .success(unsafeBitCast(create, to: Create.self))
            } else {
                result = .failure(ExactNativeRefusal("\(file): no exact_package_create entry"))
            }
        } else {
            result = .failure(ExactNativeRefusal("dlopen \(file): \(String(cString: dlerror()))"))
        }
        // A library, including one refused here, stays mapped for the process's life.
        entries[name] = result
        return result
    }

    static func adapter(_ m: ExactModule, _ name: String, module: String, session: String) -> ExactExtensionAdapter {
        switch entry(module) {
        case .failure(let error):
            m.context.diagnostics.log("\(name): \(error)")
            return RefusedPackageAdapter(String(describing: error))
        case .success(let create):
            let context = ExactExtensions.context(m, name, session: session)
            ExactExtensions.prepareContext(m, name, context)
            let raw = create(Unmanaged.passUnretained(context).toOpaque())
            let implementation = Unmanaged<ExactPackageImplementation>.fromOpaque(raw).takeRetainedValue()
            return LoadedPackageAdapter(implementation, context)
        }
    }
}

private final class RefusedPackageAdapter: ExactExtensionAdapter {
    let message: String
    init(_ message: String) { self.message = message }
    private var answer: [String: Any] { ["error": "NotSupportedError", "message": message] }
    override func later(_ name: String, _ args: [String: Any], preferred: Any?, origin: String, request: Int?, reply: ExactReply) { reply.send(answer) }
    override func ask(_ name: String, _ args: [String: Any], answer: @escaping ([String: Any]) -> Void) { answer(self.answer) }
    override func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] { answer }
    override func view(_ tag: String, id: String, props: [String: String], events: ExactNativeEvents) throws -> ExactNativeInstance {
        throw ExactNativeRefusal("\(tag): \(message)")
    }
    override func due(_ wake: [String: Any], done: @escaping (Bool) -> Void) { done(false) }
}

private final class LoadedPackageAdapter: ExactExtensionAdapter {
    let implementation: ExactPackageImplementation
    let context: ExactPackageContext
    var impl: any ExactPackage { implementation.package }
    init(_ implementation: ExactPackageImplementation, _ context: ExactPackageContext) {
        self.implementation = implementation
        self.context = context
    }
    override func later(_ name: String, _ args: [String: Any], preferred: Any?, origin: String, request: Int?, reply: ExactReply) {
        context.origin = origin
        context.request = request
        impl.later(name, args, preferred: preferred) { reply.send($0) }
    }
    override func ask(_ name: String, _ args: [String: Any], answer: @escaping ([String: Any]) -> Void) { impl.later(name, args, preferred: nil, reply: answer) }
    override func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] { try impl.call(name, args) }
    override func release(_ name: String) { impl.release(name) }
    override func release(_ name: String, args: [String: Any]) { impl.release(name, args: args) }
    override func preferred(_ values: [String: Any]) { impl.preferred(values) }
    override func dispose(_ ids: [String]) { impl.dispose(ids) }
    override func cancel(_ request: Int) { impl.cancel(request) }
    override func foreground(_ shown: Bool) {
        context.foreground = shown
        impl.foreground(shown)
    }
    override func destroy(_ done: @escaping () -> Void) { impl.destroy(done) }
    override func view(_ tag: String, id: String, props: [String: String], events: ExactNativeEvents) throws -> ExactNativeInstance {
        let sent = ExactPackageEvents(send: { kind, text in events.send(kind, text) }, size: { events.intrinsicSize($0) })
        return PackageView(try impl.view(tag, id: id, props: props, events: sent), events: events)
    }
    override func displayed(_ id: String, _ shown: Bool, origin: String) { impl.displayed(id, shown, origin: origin) }
    override func bound(_ id: String, _ prop: String, origin: String) { impl.bound(id, prop, origin: origin) }
    override func image(_ source: String, width: Int, height: Int, fit: String, owned: Bool, reply: @escaping (CGImage?, String?) -> Void) { impl.image(source, width: width, height: height, fit: fit, owned: owned, reply: reply) }
    override func permissions(_ states: [String: String]) { impl.permissions(states) }
    override func displayed(surface: String, _ shown: Bool, origin: String) { impl.displayed(surface: surface, shown, origin: origin) }
    override func orientation(_ angle: Double) { impl.orientation(angle) }
    override func due(_ wake: [String: Any], done: @escaping (Bool) -> Void) { impl.woke(ExactWake(wake, context: context, done: done)) }
    override func evaluate(_ accessControl: SecAccessControl, _ request: [String: Any], reply: @escaping (AnyObject?, [String: Any]?) -> Void) {
        guard let authenticator = implementation.authenticator else { return super.evaluate(accessControl, request, reply: reply) }
        let origin = request["origin"] as? String ?? "app"
        context.origin = origin
        authenticator.evaluate(accessControl: accessControl, reason: request["reason"] as? String ?? "", policy: request["policy"] as? String ?? "", origin: origin, reply: reply)
    }
}

private final class PackageView: ExactNativeInstance {
    let made: any ExactPackageView
    init(_ made: any ExactPackageView, events: ExactNativeEvents) { self.made = made; super.init(events: events) }
    override var view: ExactNativeView { made.view }
    #if os(iOS) || os(tvOS)
    override var controller: UIViewController? { made.controller }
    #endif
    override func setProps(_ props: [String: String]) throws { try made.setProps(props) }
    override func destroy() { made.destroy() }
}
#endif
