// The web arm and package exchange native records on the main thread.
private extension BrowserAutomationModule {
    static func checkOverflow() {
        let queue = ScriptMessageQueue()
        let start = ScriptMessageQueue.Start(handler: "player", view: ObjectIdentifier(queue), turn: "playback")
        var reports = 0, drops = 0, values: [Int] = []
        var enqueue: ((Int) -> Void)!
        let context = ExactPackageContext(agent: false, session: "player", changed: { _ in }, log: { _ in }, now: { 0 },
                                          invoke: { _, _, args, done in
                                              if let params = args["params"] as? [String: Any], let n = params["n"] as? Int { values.append(n) }
                                              if args["retiredPost"] as? Bool == true { drops += 1 }
                                              if args["overflow"] as? Bool == true { reports += 1; enqueue(33) }
                                              done([:])
                                          })
        let player = BrowserAutomationModule(context: context)
        enqueue = { n in
            queue.admit(start, ["n": n]) { event in player.post(event as! [String: Any]) }
        }
        for _ in 0..<2 { player.post(["retiredPost": true]) }
        precondition(drops == 1 && values.isEmpty)
        for n in 0...32 { enqueue(n) }
        for n in 34...40 { enqueue(n) }
        queue.publish(start)
        let deadline = Date(timeIntervalSinceNow: 10)
        while values.last != 40 && Date() < deadline { RunLoop.main.run(mode: .default, before: deadline) }
        precondition(values == Array(9...40) && reports == 1)
    }

    static func checkHeldQueue() {
        let queue = ScriptMessageQueue()
        let start = ScriptMessageQueue.Start(handler: "player", view: ObjectIdentifier(queue), turn: "playback")
        var reports = 0, values: [Int] = []
        var held: ScriptMessageQueue.Hold?
        let next = ScriptMessageQueue.Start(handler: "wallet", view: ObjectIdentifier(queue), turn: "next")
        queue.admit(start, ["n": 0]) { record in
            held = unsafeBitCast(record["holdDelivery"] as AnyObject, to: ScriptMessageQueue.Hold.self)
            held?(true)
            values.append(0)
        }
        queue.publish(start)
        for n in 1...32 { queue.admit(start, ["n": n]) { values.append($0["n"] as! Int) } }
        queue.publish(start)
        queue.admit(next, ["n": 33]) { record in
            if let n = record["n"] as? Int { values.append(n) } else { reports += 1 }
        }
        held?(false)
        queue.publish(next)
        precondition(values == [0] + Array(2...33) && reports == 1)

        values.removeAll()
        queue.admit(start, ["n": 1]) { _ in
            values.append(1)
            queue.admit(next, ["n": 3]) { _ in values.append(3) }
            queue.publish(next)
        }
        queue.admit(start, ["n": 2]) { _ in values.append(2) }
        queue.publish(start)
        precondition(values == [1, 2, 3])
    }
}

@main private enum Check {
    static func main() {
        BrowserAutomationModule.checkOverflow()
        BrowserAutomationModule.checkHeldQueue()
        let queue = ScriptMessageQueue()
        let busy = ScriptMessageQueue.Start(handler: "player", view: ObjectIdentifier(queue), turn: "busy")
        let other = ScriptMessageQueue.Start(handler: "wallet", view: ObjectIdentifier(queue), turn: "other")
        var delivered: [Int] = []
        let receive: ScriptMessageQueue.Send = { if let n = $0["n"] as? Int { delivered.append(n) } }
        queue.admit(busy, ["n": 0], at: .now() - .milliseconds(50), send: receive)
        for n in 1...32 { queue.admit(busy, ["n": n], send: receive) }
        queue.publish(other)
        precondition(delivered.first == 1, "overflow postponed the expired delivery deadline")
        queue.close()
    }
}
