import Photos
import XCTest

/// The library edits `photokit-changes.mjs` makes between the fixture's launches (LLP 1133.019.000 piece 4), through
/// PhotoKit in the test runner, which `simctl privacy` grants. `$REACH_OUT/edits.json` lists them, each
/// `{"edit": "favorite" | "unfavorite" | "hide" | "delete", "ids": [localIdentifier]}` or
/// `{"edit": "import", "files": [path]}`; the ids each edit reached or made are written to
/// `$REACH_OUT/edited.json`. Where the system asks the person first, the test answers its alert.
final class PhotoChangesUITests: XCTestCase {
    func testEdit() throws {
        let out = URL(fileURLWithPath: ProcessInfo.processInfo.environment["REACH_OUT"]!)
        let edits = try JSONSerialization.jsonObject(with: Data(contentsOf: out.appendingPathComponent("edits.json"))) as! [[String: Any]]
        XCTAssertEqual(PHPhotoLibrary.authorizationStatus(for: .readWrite), .authorized, "grant the runner photos with simctl privacy")
        let alerts = ["com.apple.springboard", Bundle.main.bundleIdentifier!].map(XCUIApplication.init(bundleIdentifier:))
        var reached: [[String]] = []
        for edit in edits {
            let word = edit["edit"] as! String, ids = edit["ids"] as? [String] ?? []
            if word == "import" {
                var made: [String] = []
                try PHPhotoLibrary.shared().performChangesAndWait {
                    for file in edit["files"] as! [String] {
                        made += [PHAssetChangeRequest.creationRequestForAssetFromImage(atFileURL: URL(fileURLWithPath: file))?.placeholderForCreatedAsset?.localIdentifier].compactMap { $0 }
                    }
                }
                reached.append(made)
                continue
            }
            let found = PHAsset.fetchAssets(withLocalIdentifiers: ids, options: nil)
            let assets = found.objects(at: IndexSet(integersIn: 0..<found.count))
            XCTAssertEqual(assets.count, ids.count, "\(word): photos not found")
            reached.append(assets.map(\.localIdentifier))
            var finished = false
            PHPhotoLibrary.shared().performChanges({
                if word == "delete" { return PHAssetChangeRequest.deleteAssets(assets as NSArray) }
                for asset in assets {
                    let request = PHAssetChangeRequest(for: asset)
                    switch word {
                    case "favorite", "unfavorite": request.isFavorite = word == "favorite"
                    default: request.isHidden = true
                    }
                }
            }) { ok, error in
                DispatchQueue.main.async {
                    XCTAssertTrue(ok, "\(word): \(String(describing: error))")
                    finished = true
                }
            }
            // The system's confirmation, where it asks one: its affirmative button, whichever process shows it.
            let end = Date().addingTimeInterval(30)
            while !finished, Date() < end {
                for app in alerts where app.alerts.firstMatch.exists {
                    let yes = app.alerts.firstMatch.buttons.matching(NSPredicate(format: "NOT (label IN %@)", ["Cancel", "Don’t Allow", "Don't Allow"])).firstMatch
                    if yes.exists { yes.tap() }
                }
                RunLoop.current.run(until: Date().addingTimeInterval(0.25))
            }
            XCTAssertTrue(finished, "\(word): unfinished\n\(alerts.map(\.debugDescription).joined(separator: "\n"))")
        }
        try JSONSerialization.data(withJSONObject: reached).write(to: out.appendingPathComponent("edited.json"))
    }
}
