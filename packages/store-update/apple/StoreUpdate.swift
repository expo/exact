// @exact/store-update on Apple (LLP 1133.038 D3): what the App Store's catalogue lists for the person's storefront,
// and the app's page in the App Store app. Apple has no API that installs or offers an update inside an app, and none
// that answers for this device, so a newer listing is `listed`, never `offered`. Not Exact's own signed updates
// (LLP 1030).
//
// The store is asked only for an install it made: one whose bundle carries the App Store's production receipt. A
// development, TestFlight or Developer ID build is not the store's, and the store's listing of its bundle id is
// another build. The lookup is the iTunes Search API's, by bundle id, for StoreKit's storefront; it is the module's
// own request, and an answer that found the app or did not is kept a minute (a failure is not kept).
//
// Under the agent nothing is asked: the status is a store install with nothing newer, with the drive's preferred
// status laid over it, and `openStorePage` journals the page it would open.
import ExactPackage
import Foundation
import StoreKit
#if os(macOS)
import AppKit
#else
import UIKit
#endif

public final class StoreUpdateModule: ExactPackage {
    static let topic = "store-update.storeUpdateStatus"
    let context: ExactPackageContext
    /// Under the agent, the drive's preferred status.
    private var laid: [String: Any] = [:]
    /// The last lookup's answer and when it was made, on the session's clock.
    private var looked: (at: Double, answer: Lookup)?
    private var waiting: [(Lookup) -> Void] = []

    public init(context: ExactPackageContext) { self.context = context }

    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        switch name {
        case "storeUpdateStatus": context.agent ? reply(agentStatus()) : status(reply)
        case "openStorePage": open(reply)
        default: reply(["error": "NotSupportedError"])
        }
    }

    public func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] { ["error": "NotSupportedError"] }

    public func preferred(_ values: [String: Any]) {
        laid = values["storeUpdateStatus"] as? [String: Any] ?? [:]
        context.changed(Self.topic)
    }

    public func foreground(_ shown: Bool) { if shown { context.changed(Self.topic) } }

    // MARK: The answer

    /// What a lookup found: the listing, none, or a failure to ask.
    enum Lookup {
        case listed([String: Any])
        case notListed
        case failed(String)
    }

    /// Whether the App Store made this install: its production receipt is in the bundle (`receipt`; TestFlight's is
    /// `sandboxReceipt`, and a development or Developer ID build has none).
    static func storeInstall(_ receipt: URL?) -> Bool {
        guard let receipt, receipt.lastPathComponent == "receipt" else { return false }
        return FileManager.default.fileExists(atPath: receipt.path)
    }

    private var running: String { Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "" }

    private func status(_ reply: @escaping ([String: Any]) -> Void) {
        guard Self.storeInstall(Bundle.main.appStoreReceiptURL) else {
            return reply(["available": false, "updateAvailability": "unknown", "apple": NSNull(), "android": NSNull()])
        }
        lookup { [running] found in
            reply(Self.judge(found, running: running, os: ProcessInfo.processInfo.operatingSystemVersion,
                             kind: Self.kind, bundleId: Bundle.main.bundleIdentifier ?? ""))
        }
    }

    /// The lookup's `kind` for this platform's binary. A universal purchase answers one record, of kind `software`,
    /// whose version and minimum OS are the iOS binary's: on the Mac it says nothing of the Mac binary.
    #if os(macOS)
    static let kind = "mac-software"
    #else
    static let kind = "software"
    #endif

    /// The status for a lookup's answer against the running version, compared only when the record is this
    /// platform's binary (`kind`) of this bundle id.
    static func judge(_ found: Lookup, running: String, os: OperatingSystemVersion, kind: String, bundleId: String) -> [String: Any] {
        var apple: [String: Any] = ["lookup": "failed", "version": NSNull(), "minimumOsVersion": NSNull(),
                                    "releaseNotes": NSNull(), "currentVersionReleaseDate": NSNull()]
        var availability = "unknown"
        switch found {
        case .failed: break
        case .notListed: apple["lookup"] = "not-found"
        case .listed(let listing):
            apple["lookup"] = "found"
            guard listing["kind"] as? String == kind, listing["bundleId"] as? String == bundleId else { break }
            for key in ["version", "minimumOsVersion", "releaseNotes", "currentVersionReleaseDate"] {
                if let value = listing[key] as? String { apple[key] = value }
            }
            let version = listing["version"] as? String ?? ""
            let osVersion = "\(os.majorVersion).\(os.minorVersion).\(os.patchVersion)"
            // A version that is not period-separated integers cannot be compared: `unknown`.
            guard let newer = compare(version, running) else { break }
            let installable = (listing["minimumOsVersion"] as? String).flatMap { compare($0, osVersion) }.map { $0 <= 0 } ?? true
            availability = newer > 0 && installable ? "listed" : "current"
        }
        return ["available": true, "updateAvailability": availability, "apple": apple, "android": NSNull()]
    }

    /// Two versions as period-separated integers, missing parts zero: negative, zero or positive; nil when either
    /// is not of that form.
    static func compare(_ a: String, _ b: String) -> Int? {
        let x = a.split(separator: ".", omittingEmptySubsequences: false).map { Int($0) }
        let y = b.split(separator: ".", omittingEmptySubsequences: false).map { Int($0) }
        guard !a.isEmpty, !b.isEmpty, !x.contains(nil), !y.contains(nil) else { return nil }
        let l0 = x.compactMap { $0 }, r0 = y.compactMap { $0 }
        for i in 0..<max(l0.count, r0.count) {
            let l = i < l0.count ? l0[i] : 0, r = i < r0.count ? r0[i] : 0
            if l != r { return l < r ? -1 : 1 }
        }
        return 0
    }

    /// A store install with nothing newer, the drive's preferred fields laid over it (`apple`'s field by field).
    private func agentStatus() -> [String: Any] {
        var apple: [String: Any] = ["lookup": "found", "version": running, "minimumOsVersion": NSNull(),
                                    "releaseNotes": NSNull(), "currentVersionReleaseDate": NSNull()]
        var out: [String: Any] = ["available": true, "updateAvailability": "current", "android": NSNull()]
        for (key, value) in laid {
            if key == "apple", let fields = value as? [String: Any] { apple.merge(fields) { _, new in new } } else { out[key] = value }
        }
        if out["apple"] == nil { out["apple"] = apple }
        return out
    }

    // MARK: The lookup

    /// ISO 3166-1 alpha-3 to alpha-2, five letters each: StoreKit's storefront code is alpha-3, and the lookup takes
    /// alpha-2 only. From ICU's table, with Apple's storefront code for Kosovo.
    static let countries: [String: String] = {
        let table = """
            ABWAW AFGAF AGOAO AIAAI ALAAX ALBAL ANDAD AREAE ARGAR ARMAM ASCAC ASMAS ATAAQ ATFTF ATGAG AUSAU AUTAT AZEAZ
            BDIBI BELBE BENBJ BESBQ BFABF BGDBD BGRBG BHRBH BHSBS BIHBA BLMBL BLRBY BLZBZ BMUBM BOLBO BRABR BRBBB BRNBN
            BTNBT BVTBV BWABW CAFCF CANCA CCKCC CHECH CHLCL CHNCN CIVCI CMRCM CODCD COGCG COKCK COLCO COMKM CPTCP CPVCV
            CRICR CRQCQ CUBCU CUWCW CXRCX CYMKY CYPCY CZECZ DEUDE DGADG DJIDJ DMADM DNKDK DOMDO DZADZ ECUEC EGYEG ERIER
            ESHEH ESPES ESTEE ETHET FINFI FJIFJ FLKFK FRAFR FROFO FSMFM GABGA GBRGB GEOGE GGYGG GHAGH GIBGI GINGN GLPGP
            GMBGM GNBGW GNQGQ GRCGR GRDGD GRLGL GTMGT GUFGF GUMGU GUYGY HKGHK HMDHM HNDHN HRVHR HTIHT HUNHU IDNID IMNIM
            INDIN IOTIO IRLIE IRNIR IRQIQ ISLIS ISRIL ITAIT JAMJM JEYJE JORJO JPNJP KAZKZ KENKE KGZKG KHMKH KIRKI KNAKN
            KORKR KWTKW LAOLA LBNLB LBRLR LBYLY LCALC LIELI LKALK LSOLS LTULT LUXLU LVALV MACMO MAFMF MARMA MCOMC MDAMD
            MDGMG MDVMV MEXMX MHLMH MKDMK MLIML MLTMT MMRMM MNEME MNGMN MNPMP MOZMZ MRTMR MSRMS MTQMQ MUSMU MWIMW MYSMY
            MYTYT NAMNA NCLNC NERNE NFKNF NGANG NICNI NIUNU NLDNL NORNO NPLNP NRUNR NZLNZ OMNOM PAKPK PANPA PCNPN PERPE
            PHLPH PLWPW PNGPG POLPL PRIPR PRKKP PRTPT PRYPY PSEPS PYFPF QATQA REURE ROURO RUSRU RWARW SAUSA SDNSD SENSN
            SGPSG SGSGS SHNSH SJMSJ SLBSB SLESL SLVSV SMRSM SOMSO SPMPM SRBRS SSDSS STPST SURSR SVKSK SVNSI SWESE SWZSZ
            SXMSX SYCSC SYRSY TAATA TCATC TCDTD TGOTG THATH TJKTJ TKLTK TKMTM TLSTL TONTO TTOTT TUNTN TURTR TUVTV TWNTW
            TZATZ UGAUG UKRUA UMIUM URYUY USAUS UZBUZ VATVA VCTVC VENVE VGBVG VIRVI VNMVN VUTVU WLFWF WSMWS XEAEA XICIC
            XKKXK YEMYE ZAFZA ZMBZM ZWEZW XKSXK
            """
        var out: [String: String] = [:]
        for entry in table.split(whereSeparator: \.isWhitespace) where entry.count == 5 { out[String(entry.prefix(3))] = String(entry.suffix(2)) }
        return out
    }()

    static func lookupURL(bundleId: String, country: String) -> URL? {
        var parts = URLComponents(string: "https://itunes.apple.com/lookup")
        parts?.queryItems = [URLQueryItem(name: "bundleId", value: bundleId), URLQueryItem(name: "country", value: country.lowercased())]
        return parts?.url
    }

    /// The lookup's answer for `data`: its first result, or none.
    static func parse(_ data: Data) -> Lookup {
        guard let body = try? JSONSerialization.jsonObject(with: data) as? [String: Any], let results = body["results"] as? [[String: Any]] else {
            return .failed("the lookup's answer is not the iTunes Search API's")
        }
        switch results.count {
        case 0: return .notListed
        case 1: return .listed(results[0])
        default: return .failed("the lookup answered \(results.count) records for one bundle id")
        }
    }

    /// The App Store's listing of this app for StoreKit's storefront; a lookup inside the last minute answers again.
    private func lookup(_ done: @escaping (Lookup) -> Void) {
        if let looked, context.now() - looked.at < 60_000 { return done(looked.answer) }
        waiting.append(done)
        guard waiting.count == 1 else { return }
        let finish: (Lookup) -> Void = { [weak self] answer in
            DispatchQueue.main.async {
                guard let self else { return }
                if case .failed(let why) = answer { self.context.log("store-update: lookup failed: \(why)") } else { self.looked = (self.context.now(), answer) }
                let all = self.waiting
                self.waiting = []
                all.forEach { $0(answer) }
            }
        }
        Task {
            guard let code = await Storefront.current?.countryCode else { return finish(.failed("no App Store storefront")) }
            guard let country = Self.countries[code] else { return finish(.failed("storefront \(code) has no two-letter code")) }
            guard let id = Bundle.main.bundleIdentifier, let url = Self.lookupURL(bundleId: id, country: country) else {
                return finish(.failed("no bundle id"))
            }
            do {
                let (data, response) = try await URLSession.shared.data(from: url)
                guard (response as? HTTPURLResponse)?.statusCode == 200 else { return finish(.failed("the lookup answered \((response as? HTTPURLResponse)?.statusCode ?? 0)")) }
                finish(Self.parse(data))
            } catch {
                finish(.failed(error.localizedDescription))
            }
        }
    }

    // MARK: The store page

    static func storePage(_ trackId: Int) -> URL? {
        #if os(macOS)
        URL(string: "macappstore://apps.apple.com/app/id\(trackId)")
        #else
        URL(string: "itms-apps://apps.apple.com/app/id\(trackId)")
        #endif
    }

    private func open(_ reply: @escaping ([String: Any]) -> Void) {
        if context.agent {
            context.log("openStorePage: the App Store page (\(context.origin))")
            return reply([:])
        }
        guard Self.storeInstall(Bundle.main.appStoreReceiptURL) else {
            return reply(["error": "NotSupportedError", "message": "this install is not from the App Store"])
        }
        lookup { [weak self] found in
            guard let self else { return }
            guard case .listed(let listing) = found, let id = listing["trackId"] as? Int, let url = Self.storePage(id) else {
                return reply(["error": "NotFoundError", "message": "the App Store does not list this app in this storefront"])
            }
            guard self.context.leave(url.absoluteString, how: "external") else {
                return reply(["error": "NotAllowedError", "message": "the embedder declined to leave the app for the store"])
            }
            #if os(macOS)
            let opened = NSWorkspace.shared.open(url)
            reply(opened ? [:] : ["error": "OperationError", "message": "the App Store did not open"])
            #else
            UIApplication.shared.open(url) { opened in
                reply(opened ? [:] : ["error": "OperationError", "message": "the App Store did not open"])
            }
            #endif
        }
    }
}
