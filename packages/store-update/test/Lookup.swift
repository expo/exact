// Appended to StoreUpdate.swift by lookup.test.mjs: the App Store's answer judged against the running version, on
// canned answers in the iTunes Search API's form. With EXACT_STORE_LOOKUP=<bundle id> it also asks the real API for
// that app, as a person checks by hand; the suite never does.
private extension StoreUpdateModule {
    static func testLookup() -> [String] {
        var failures: [String] = []
        func check(_ ok: Bool, _ what: String) { if !ok { failures.append(what) } }
        let ios = OperatingSystemVersion(majorVersion: 27, minorVersion: 0, patchVersion: 0)
        let listing = Data("""
            {"resultCount":1,"results":[{"kind":"software","bundleId":"com.example.app","version":"15.4.2","trackId":361309726,"minimumOsVersion":"26.0",
            "releaseNotes":"Fixes.","currentVersionReleaseDate":"2026-10-23T03:50:59Z"}]}
            """.utf8)
        let found = parse(listing)
        guard case .listed(let fields) = found else { return ["a listing parses as listed"] }
        check(fields["trackId"] as? Int == 361309726, "the track id is read")

        let newer = judge(found, running: "15.4.1", os: ios, kind: "software", bundleId: "com.example.app")
        check(newer["available"] as? Bool == true, "a store install is available")
        check(newer["updateAvailability"] as? String == "listed", "a higher listed version is listed, never offered")
        let apple = newer["apple"] as? [String: Any] ?? [:]
        check(apple["lookup"] as? String == "found" && apple["version"] as? String == "15.4.2", "the listing's version is kept")
        check(apple["releaseNotes"] as? String == "Fixes." && apple["minimumOsVersion"] as? String == "26.0", "notes and minimum OS are kept")
        check(judge(found, running: "15.4.2", os: ios, kind: "software", bundleId: "com.example.app")["updateAvailability"] as? String == "current", "the same version is not newer")
        check(judge(found, running: "15.10", os: ios, kind: "software", bundleId: "com.example.app")["updateAvailability"] as? String == "current", "a build ahead of the store is not behind it")

        let old = OperatingSystemVersion(majorVersion: 25, minorVersion: 4, patchVersion: 0)
        check(judge(found, running: "15.0", os: old, kind: "software", bundleId: "com.example.app")["updateAvailability"] as? String == "current", "an iOS below the listing's minimum is not offered it")
        let mac = judge(found, running: "1.0", os: ios, kind: "mac-software", bundleId: "com.example.app")
        check(mac["updateAvailability"] as? String == "unknown" && (mac["apple"] as? [String: Any])?["version"] is NSNull,
              "a universal purchase's iOS record says nothing of the Mac binary")
        check(judge(found, running: "1.0", os: ios, kind: "software", bundleId: "com.example.other")["updateAvailability"] as? String == "unknown",
              "another bundle id's record is not this app's")
        let macOnly = parse(Data(#"{"results":[{"kind":"mac-software","bundleId":"com.example.app","version":"3.1","trackId":1,"minimumOsVersion":"14.0"}]}"#.utf8))
        check(judge(macOnly, running: "3.0", os: ios, kind: "mac-software", bundleId: "com.example.app")["updateAvailability"] as? String == "listed",
              "a Mac record is compared on the Mac")

        let none = judge(parse(Data(#"{"resultCount":0,"results":[]}"#.utf8)), running: "1.0", os: ios, kind: "software", bundleId: "com.example.app")
        check(none["updateAvailability"] as? String == "unknown" && (none["apple"] as? [String: Any])?["lookup"] as? String == "not-found", "an unlisted app is unknown, not-found")
        let broken = judge(parse(Data("{\"errorMessage\":\"Invalid value(s) for key(s): [country]\"}".utf8)), running: "1.0", os: ios, kind: "software", bundleId: "com.example.app")
        check((broken["apple"] as? [String: Any])?["lookup"] as? String == "failed", "an error answer is failed")

        check(compare("1.10", "1.9")! > 0 && compare("2", "2.0.0") == 0 && compare("1.2", "1.2.1")! < 0, "versions compare as integers, missing parts zero")
        check(compare("1.0b2", "1.0") == nil && compare("", "1") == nil && compare("1..2", "1") == nil, "other versions do not compare")
        check(judge(found, running: "15.4-dev", os: ios, kind: "software", bundleId: "com.example.app")["updateAvailability"] as? String == "unknown",
              "a running version that does not compare is unknown")
        let iosOld = judge(found, running: "15.0", os: old, kind: "software", bundleId: "com.example.app")["apple"] as? [String: Any]
        check(iosOld?["minimumOsVersion"] as? String == "26.0" && iosOld?["version"] as? String == "15.4.2", "an uninstallable listing still says what it needs")
        check(countries["USA"] == "US" && countries["JPN"] == "JP" && countries["GBR"] == "GB" && countries["XKS"] == "XK", "storefront codes map to two letters")
        check(lookupURL(bundleId: "com.example.app", country: "US")?.absoluteString == "https://itunes.apple.com/lookup?bundleId=com.example.app&country=us", "the lookup URL")
        check(storePage(361309726)?.absoluteString == "macappstore://apps.apple.com/app/id361309726", "the Mac opens the Mac App Store")

        let missing = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("no-such/receipt")
        check(!storeInstall(nil) && !storeInstall(missing), "no receipt is not a store install")
        let dir = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("store-update-\(getpid())")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let production = dir.appendingPathComponent("receipt"), sandbox = dir.appendingPathComponent("sandboxReceipt")
        FileManager.default.createFile(atPath: production.path, contents: Data())
        FileManager.default.createFile(atPath: sandbox.path, contents: Data())
        check(storeInstall(production), "the production receipt is a store install")
        check(!storeInstall(sandbox), "TestFlight's sandbox receipt is not")
        try? FileManager.default.removeItem(at: dir)
        return failures
    }

    /// The real API, for `bundleId` in `country`, judged against `running`.
    static func askReal(_ bundleId: String, country: String, running: String) -> [String: Any] {
        let done = DispatchSemaphore(value: 0)
        var answer: Lookup = .failed("no answer")
        URLSession.shared.dataTask(with: lookupURL(bundleId: bundleId, country: country)!) { data, _, error in
            answer = data.map(parse) ?? .failed(error?.localizedDescription ?? "no data")
            done.signal()
        }.resume()
        done.wait()
        return judge(answer, running: running, os: ProcessInfo.processInfo.operatingSystemVersion, kind: "software", bundleId: bundleId)
    }
}

@main private struct LookupTests {
    static func main() {
        let failures = StoreUpdateModule.testLookup()
        failures.forEach { print("FAIL \($0)") }
        print("store-update lookup: \(failures.count) failures")
        if let id = ProcessInfo.processInfo.environment["EXACT_STORE_LOOKUP"] {
            let real = StoreUpdateModule.askReal(id, country: ProcessInfo.processInfo.environment["EXACT_STORE_COUNTRY"] ?? "us",
                                                 running: ProcessInfo.processInfo.environment["EXACT_STORE_RUNNING"] ?? "1.0")
            print("real lookup of \(id): \(real)")
        }
        exit(failures.isEmpty ? 0 : 1)
    }
}
