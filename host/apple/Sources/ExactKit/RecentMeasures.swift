// Answers a session can measure again, kept within a budget of entries and
// bytes. Two generations: a lookup that finds an entry in the older moves it
// to the newer, and when the newer passes its budget it becomes the older and
// the previous older is released. What layouts keep asking for survives each
// turn; what none asks for any more (a count's old title, a width a resize
// passed) is released within two budgets' worth of new entries.
struct RecentMeasures<Key: Hashable, Value> {
    private var newer: [Key: Value] = [:], older: [Key: Value] = [:]
    private var newerBytes = 0, olderBytes = 0
    private let entries: Int, bytes: Int
    private let cost: (Key, Value) -> Int

    /// `cost` is an entry's bytes beyond its fixed size: its key's face, a title.
    init(entries: Int, bytes: Int, cost: @escaping (Key, Value) -> Int) {
        self.entries = entries; self.bytes = bytes; self.cost = cost
    }

    mutating func value(_ key: Key) -> Value? {
        if let value = newer[key] { return value }
        guard let value = older.removeValue(forKey: key) else { return nil }
        olderBytes -= cost(key, value)
        insert(key, value)
        return value
    }

    mutating func insert(_ key: Key, _ value: Value) {
        if let old = older.removeValue(forKey: key) { olderBytes -= cost(key, old) }
        if let old = newer.updateValue(value, forKey: key) { newerBytes -= cost(key, old) }
        newerBytes += cost(key, value)
        if newer.count > entries || newerBytes > bytes {
            older = newer; olderBytes = newerBytes
            newer = [:]; newerBytes = 0
        }
    }

    mutating func removeAll() {
        newer = [:]; older = [:]
        newerBytes = 0; olderBytes = 0
    }

    /// What is held: entries, and their bytes as `cost` counts them.
    var retained: (entries: Int, bytes: Int) { (newer.count + older.count, newerBytes + olderBytes) }
}
