// Compares Caltrain's agent push with a UIKit-only screen on the same simulator (LLP 1172.001 §4 step 3).
// Compiled into both apps for measurement only. The endpoint is the run-loop observer after Core Animation's
// before-waiting commit; it is submission, not display presentation or animation completion.
import UIKit
import os.signpost

final class PushProbe {
    static let shared = PushProbe()
    let log = OSLog(subsystem: "dev.exact.bareref", category: .pointsOfInterest)
    var serial = 0
    var active = false
    var records: [[String: Any]] = []
    var parts: [String: Double] = [:]
    var applies: [[String: Any]] = []
    var completed: (() -> Void)?

    func cpu() -> Double {
        var info = thread_basic_info()
        var count = mach_msg_type_number_t(MemoryLayout<thread_basic_info>.size / MemoryLayout<natural_t>.size)
        let port = mach_thread_self()
        defer { mach_port_deallocate(mach_task_self_, port) }
        let result = withUnsafeMutablePointer(to: &info) { pointer in
            pointer.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                thread_info(port, thread_flavor_t(THREAD_BASIC_INFO), $0, &count)
            }
        }
        precondition(result == KERN_SUCCESS)
        return Double(info.user_time.seconds + info.system_time.seconds) * 1000
            + Double(info.user_time.microseconds + info.system_time.microseconds) / 1000
    }

    @inline(never) func begin() {
        precondition(Thread.isMainThread && !active)
        serial += 1
        active = true
        applies = []
        let id = OSSignpostID(log: log)
        os_signpost(.begin, log: log, name: "PushFirstCommit", signpostID: id, "%d", serial)
        let startCPU = cpu(), start = CACurrentMediaTime()
        let observer = CFRunLoopObserverCreateWithHandler(nil, CFRunLoopActivity.beforeWaiting.rawValue,
                                                        false, 2_000_001) { [self] _, _ in
            let end = CACurrentMediaTime(), endCPU = cpu()
            os_signpost(.end, log: log, name: "PushFirstCommit", signpostID: id, "%d", serial)
            active = false
            records.append(["serial": serial, "start": start, "end": end,
                            "wallMs": (end - start) * 1000, "cpuMs": endCPU - startCPU, "applies": applies])
            publish()
            completed?()
            completed = nil
        }
        CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .commonModes)
    }

    @inline(never) func publish() {
        let url = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("bareref.json")
        try! JSONSerialization.data(withJSONObject: records).write(to: url, options: .atomic)
        if serial == 1 {
            let windows = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }.flatMap { $0.windows }
            try! JSONSerialization.data(withJSONObject: windows.map(Self.hierarchy)).write(
                to: url.deletingLastPathComponent().appendingPathComponent("bareref-views.json"))
        }
    }

    static func hierarchy(_ view: UIView) -> [String: Any] {
        var row: [String: Any] = ["class": String(describing: type(of: view)),
                                 "frame": [view.frame.minX, view.frame.minY, view.frame.width, view.frame.height],
                                 "hidden": view.isHidden, "alpha": view.alpha,
                                 "children": view.subviews.map(hierarchy)]
        if let label = view as? UILabel { row["text"] = label.text ?? "" }
        if let field = view as? UITextField { row["placeholder"] = field.placeholder ?? "" }
        row["id"] = view.accessibilityIdentifier ?? ""
        return row
    }
}
