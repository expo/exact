// @ref LLP 1115.002 §4. The collection windows by item. AppKit addresses
// each header and body row inside those items by one flat row number.
#if os(macOS)
import Foundation

struct WindowedTableSections {
    struct Run {
        let section: Int, row: Int, headers: [Bool]
    }

    final class Shape {
        let runs: [Run]
        let count: Int
        init?(_ entries: [BatchValue]) {
            var runs: [Run] = [], section = 0, row = 0
            for entry in entries {
                guard case .array(let fields) = entry, fields.count == 2,
                      let end = fields[0].number.flatMap(Int.init(exactly:)), end > section,
                      case .array(let values) = fields[1] else { return nil }
                let headers: [Bool] = values.compactMap { if case .bool(let flag) = $0 { return flag }; return nil }
                guard headers.count == values.count else { return nil }
                runs.append(Run(section: section, row: row, headers: headers))
                row += (end - section) * headers.count
                section = end
            }
            self.runs = runs
            count = row
        }
    }

    let shape: Shape
    var count: Int { shape.count }
    let mounted: [(section: Int, cells: [UInt32])]

    init?(_ fields: [BatchValue], previous: Self?) {
        guard fields.count == 3, case .array(let items) = fields[2] else { return nil }
        switch fields[1] {
        case .array(let runs):
            guard let shape = Shape(runs) else { return nil }
            self.shape = shape
        case .null:
            guard let previous else { return nil }
            shape = previous.shape
        default: return nil
        }
        var mounted: [(Int, [UInt32])] = []
        for item in items {
            guard case .array(let pair) = item, pair.count == 2,
                  let index = pair[0].number.flatMap(Int.init(exactly:)), case .array(let views) = pair[1] else { return nil }
            let cells = views.compactMap { $0.number.flatMap(UInt32.init(exactly:)) }
            guard cells.count == views.count else { return nil }
            mounted.append((index, cells))
        }
        self.mounted = mounted
    }

    private func run(at value: Int, by key: KeyPath<Run, Int>) -> Run? {
        let runs = shape.runs
        var lo = 0, hi = runs.count
        while lo < hi {
            let mid = lo + (hi - lo) / 2
            if runs[mid][keyPath: key] <= value { lo = mid + 1 } else { hi = mid }
        }
        return lo > 0 ? runs[lo - 1] : nil
    }

    func firstRow(of section: Int) -> Int {
        guard let run = run(at: section, by: \.section) else { return 0 }
        return run.row + (section - run.section) * run.headers.count
    }

    func isHeader(_ row: Int) -> Bool {
        guard row >= 0, row < count, let run = run(at: row, by: \.row), !run.headers.isEmpty else { return false }
        return run.headers[(row - run.row) % run.headers.count]
    }
}
#endif
