// Appended to Translation.swift by downloads.test.mjs: one download sheet a pair, which every create
// for that pair waits on, without a window or the Translation framework's sheet.
private extension TranslationModule {
    @MainActor
    static func testDownloads() -> [String] {
        var failures: [String] = []
        func check(_ what: String, _ ok: Bool, _ got: Any = "") { if !ok { failures.append("\(what): got \(got)") } }
        let context = ExactPackageContext(agent: true, session: "downloads", changed: { _ in }, log: { _ in }, now: { 0 })

        // The bookkeeping `download` keeps: a second create joins the first's sheet.
        let m = TranslationModule(context: context)
        var heard: [String] = []
        check("the first create starts the sheet", m.join("en>fr") { heard.append("a \($0?["error"] ?? "ok")") })
        check("a second create for the pair joins it", !m.join("en>fr") { heard.append("b \($0?["error"] ?? "ok")") })
        check("another pair starts its own", m.join("en>de") { heard.append("c \($0?["error"] ?? "ok")") })
        m.settle("en>fr", ["error": "NotAllowedError", "message": "declined"])
        check("each create waiting hears the sheet's answer once", heard == ["a NotAllowedError", "b NotAllowedError"], heard)
        m.settle("en>fr", nil)
        check("an ended download answers no one again", heard.count == 2, heard)
        check("the next create starts a new sheet", m.join("en>fr") { _ in })

        // A create that reaches `download` while the pair's sheet is up asks for no sheet of its own
        // (this process has no window to host one) and is answered with the sheet's create.
        let real = TranslationModule(context: context)
        var answers: [String] = []
        _ = real.join("en>fr") { answers.append("first \($0?["error"] ?? "ok")") }
        if #available(macOS 15.0, *) {
            real.download(Pair(source: "en", target: "fr", key: "en>fr")) { answers.append("second \($0?["error"] ?? "ok")") }
        }
        check("the second create waits for the sheet up", answers.isEmpty, answers)
        real.settle("en>fr", nil)
        check("both creates hear the sheet's answer", answers == ["first ok", "second ok"], answers)

        // Under the agent, two creates during one download are both answered when it finishes.
        let agent = TranslationModule(context: context)
        agent.preferred(["translatorAvailability": ["availability": "downloadable"]])
        var created: [[String: Any]] = []
        for _ in 0..<2 {
            agent.later("createTranslator", ["sourceLanguage": "en", "targetLanguage": "fr"], preferred: nil) { created.append($0) }
        }
        check("the creates wait for the download", created.isEmpty, created)
        agent.preferred(["translatorAvailability": ["availability": "available"]])
        check("both are made when it finishes", created.count == 2 && created.allSatisfy { $0["id"] is String }, created)
        return failures
    }
}

@main
private struct DownloadTests {
    @MainActor
    static func main() {
        let failures = TranslationModule.testDownloads()
        for failure in failures { print("FAIL: \(failure)") }
        print("translation downloads: \(failures.count) failures")
        exit(failures.isEmpty ? 0 : 1)
    }
}
