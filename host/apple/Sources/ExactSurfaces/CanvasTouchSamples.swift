import Foundation
import CoreGraphics

/// UIKit may repeat the callback touch in its coalesced batch. Keep window
/// positions here; the receiver converts each segment to current canvas coordinates.
struct CanvasTouchSamples {
    struct Sample {
        let point: CGPoint
        let timestamp: TimeInterval
    }
    private var last: Sample?

    mutating func forward(phase: String, current: Sample, coalesced: [Sample],
                          emit: (String, Sample, CGPoint) -> Void) {
        if phase == "down" {
            last = current
            emit(phase, current, .zero)
            return
        }
        let ending = phase == "up" || phase == "cancel"
        let ordered = (coalesced + (ending ? [] : [current])).enumerated().sorted {
            $0.element.timestamp == $1.element.timestamp ? $0.offset < $1.offset
                : $0.element.timestamp < $1.element.timestamp
        }
        for (_, sample) in ordered where sample.timestamp <= current.timestamp {
            if let last, sample.timestamp < last.timestamp
                || (sample.timestamp == last.timestamp && sample.point == last.point) { continue }
            send("move", sample, emit: emit)
        }
        // The final position may equal the last move, but its edge is still owed.
        if ending { send(phase, current, emit: emit) }
    }
    mutating func cancel(emit: (String, Sample, CGPoint) -> Void) {
        guard let sample = last else { return }
        last = nil
        emit("cancel", sample, .zero)
    }
    private mutating func send(_ phase: String, _ sample: Sample,
                               emit: (String, Sample, CGPoint) -> Void) {
        let from = last?.point ?? sample.point
        last = sample
        emit(phase, sample, CGPoint(x: sample.point.x - from.x, y: sample.point.y - from.y))
    }
}
