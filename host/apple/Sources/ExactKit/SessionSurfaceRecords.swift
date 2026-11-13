import Foundation

extension ExactSession {
    package func surfaceRecord(_ name: String, _ json: String?) {
        publishSurfaceRecord(name, json, performance: false)
    }

    package func canvasPerformanceRequested(_ name: String) -> Bool {
        state != .destroyed && runtime.canvasPerformanceRequested(name)
    }

    package func canvasPerformanceRecord(_ name: String, _ json: String?) {
        publishSurfaceRecord(name, json, performance: true)
    }

    private func publishSurfaceRecord(_ name: String, _ json: String?, performance: Bool) {
        guard state != .destroyed else { return }
        if applying { pendingSurfaceRecords.append((name, json, performance)); return }
        apply(performance ? runtime.canvasPerformanceRecord(name, json) : runtime.surfaceRecord(name, json))
    }
}
