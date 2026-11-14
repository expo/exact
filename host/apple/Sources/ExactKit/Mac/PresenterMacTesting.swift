#if os(macOS) && DEBUG
extension Presenter {
    /// Runs `body` as a batch's ops would run, then that batch's after-batch work (tests).
    func testInBatch(_ body: () -> Void) {
        applying = true; body(); applying = false
        let later = afterBatchWork; afterBatchWork = []
        later.forEach { $0() }
    }
}
#endif
