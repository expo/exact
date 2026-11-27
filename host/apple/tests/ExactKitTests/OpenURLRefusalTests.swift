import XCTest
@testable import ExactKit

/// A refused `openURL` names its scheme and none of the URL, whose path or
/// query can carry a token, as the painted and web hosts' refusals do.
final class OpenURLRefusalTests: XCTestCase {
    func testARefusalNamesTheSchemeAlone() {
        let cases = [
            ("unknown:/route?token=private", "unknown:"),
            (" JavaScript:alert('private')", "javascript:"),
            ("ftp://example.com/private?token=private", "ftp:"),
            ("/Users/private/gone.txt?token=private", "a URL with no scheme"),
            ("not a url: private", "a URL with no scheme"),
            ("", "a URL with no scheme"),
        ]
        for (target, named) in cases {
            let said = ExactEnv.refusedOpenName(target)
            XCTAssertEqual(said, named, target)
            XCTAssertFalse(said.contains("private"), target)
        }
    }
}
