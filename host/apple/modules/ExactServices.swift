import Foundation

extension ExactModule {
    /// Queries a service loaded by the app's `launch` configuration, such as `observe`.
    /// Calls `reply` once, on any thread, with the JSON answer or nil when the
    /// service is unavailable, does not answer queries, or `request` is not JSON.
    /// Observe accepts `["op": "recent", "limit": n]`.
    public func service(_ module: String, _ request: [String: Any], reply: @escaping ([String: Any]?) -> Void) {
        guard let hatches else { return reply(nil) }
        hatches.service(module, request, reply: reply)
    }
}

extension ExactHatches {
    typealias ServiceReplyFn = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32) -> Void
    typealias ServiceFn = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32, UnsafePointer<UInt8>?, UInt32, UnsafeMutableRawPointer?, ServiceReplyFn) -> Void
    func service(_ module: String, _ request: [String: Any], reply: @escaping ([String: Any]?) -> Void) {
        guard let serviceFn, let json = exactJSON(request) else { return reply(nil) }
        let box = Unmanaged.passRetained(ServiceReply(reply)).toOpaque()
        let name = Array(module.utf8)
        name.withUnsafeBufferPointer { n in
            json.withUnsafeBytes { j in
                serviceFn(host, n.baseAddress, UInt32(n.count), j.bindMemory(to: UInt8.self).baseAddress, UInt32(json.count), box) { context, bytes, length in
                    guard let context else { return }
                    let box = Unmanaged<ServiceReply>.fromOpaque(context).takeRetainedValue()
                    let data = bytes.map { Data(bytes: $0, count: Int(length)) } ?? Data()
                    box.reply(length == 0 ? nil : (try? JSONSerialization.jsonObject(with: data)) as? [String: Any])
                }
            }
        }
    }

    private final class ServiceReply {
        let reply: ([String: Any]?) -> Void
        init(_ reply: @escaping ([String: Any]?) -> Void) { self.reply = reply }
    }
}
