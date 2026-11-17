// UIKit-only reference for Caltrain Home → Change station. No Exact code is linked.
// The captured 119-node fixture keeps its 53 labels, 24 press targets, search field, scroll and authored bar.
import UIKit

struct Node: Decodable {
    let id: Int
    let kind: String
    let frame: [Double]
    let text: String?
    let name: String
    let font: Double
    let weight: Double
    let radius: Double
    let children: [Node]
}

final class Screen: UIViewController {
    let node: Node
    init(_ node: Node) { self.node = node; super.init(nibName: nil, bundle: nil) }
    required init?(coder: NSCoder) { fatalError() }
    override func loadView() {
        let content = make(node)
        view = UIView(frame: content.frame)
        view.addSubview(content)
    }

    @inline(never) func make(_ node: Node) -> UIView {
        let view: UIView
        switch node.kind {
        case "Text":
            let label = UILabel()
            label.text = node.text
            label.font = .systemFont(ofSize: node.font, weight: UIFont.Weight(rawValue: (node.weight - 400) / 1000))
            label.numberOfLines = 0
            view = label
        case "Pressable":
            view = UIControl()
        case "Image":
            view = UIImageView(image: UIImage(named: "caltrain"))
        default: view = UIView()
        }
        view.frame = CGRect(x: node.frame[0], y: node.frame[1], width: node.frame[2], height: node.frame[3])
        view.accessibilityIdentifier = node.name
        view.layer.cornerRadius = node.radius
        var content = view
        if node.kind == "ScrollView" {
            let scroll = UIScrollView(frame: view.bounds)
            scroll.contentInsetAdjustmentBehavior = .never
            scroll.contentSize = CGSize(width: view.bounds.width, height: 1289)
            view.addSubview(scroll)
            content = scroll
        } else if node.kind == "TextInput" {
            let field = UITextField(frame: view.bounds.insetBy(dx: 12, dy: 12))
            field.placeholder = "Search stations"
            field.font = .systemFont(ofSize: node.font)
            field.accessibilityLabel = "Search stations"
            view.layer.borderWidth = 1
            view.layer.borderColor = UIColor.separator.cgColor
            view.addSubview(field)
        } else if node.kind == "Canvas" {
            // The CPU comparison excludes Exact's GPU surfaces. Keep the view, without a renderer.
            view.addSubview(UIView(frame: view.bounds))
            content = UIView(frame: view.bounds)
            view.addSubview(content)
        }
        for child in node.children { content.addSubview(make(child)) }
        if node.name == "stations-screen" || node.kind == "View" && node.radius > 0 {
            view.backgroundColor = .secondarySystemGroupedBackground
        }
        return view
    }
}

@main
final class App: UIResponder, UIApplicationDelegate {
    var window: UIWindow?
    var navigation: UINavigationController!
    var home: UIViewController!
    var change: UIControl!
    var station: Node!
    let driver = SocketDriver()

    func application(_ application: UIApplication, didFinishLaunchingWithOptions options: [UIApplication.LaunchOptionsKey: Any]?) -> Bool {
        return true
    }

    func application(_ application: UIApplication, configurationForConnecting session: UISceneSession, options: UIScene.ConnectionOptions) -> UISceneConfiguration {
        let config = UISceneConfiguration(name: "Default", sessionRole: session.role)
        config.delegateClass = Scene.self
        return config
    }

    func setup(_ scene: UIWindowScene) {
        station = try! JSONDecoder().decode(Node.self, from: Data(contentsOf: Bundle.main.url(forResource: "stations", withExtension: "json")!))
        let homeNode = try! JSONDecoder().decode(Node.self, from: Data(contentsOf: Bundle.main.url(forResource: "home", withExtension: "json")!))
        home = Screen(homeNode)
        func find(_ view: UIView) -> UIControl? {
            if view.accessibilityIdentifier == "change-station" { return view as? UIControl }
            return view.subviews.lazy.compactMap(find).first
        }
        change = find(home.view)!
        change.addTarget(self, action: #selector(push), for: .touchUpInside)
        navigation = UINavigationController(rootViewController: home)
        _ = navigation.view
        navigation.navigationBar.addSubview(UIView())
        navigation.setNavigationBarHidden(true, animated: false)
        let host = UIViewController()
        host.view.frame = scene.coordinateSpace.bounds
        let shell = UIView(frame: host.view.bounds)
        let viewport = UIScrollView(frame: shell.bounds)
        viewport.contentInsetAdjustmentBehavior = .never
        viewport.contentSize = viewport.bounds.size
        let plain = UIView(frame: viewport.bounds)
        let main = UIView(frame: plain.bounds)
        host.view.addSubview(shell)
        shell.addSubview(viewport)
        viewport.addSubview(plain)
        plain.addSubview(main)
        // Match the retained host's view depth and empty document/head mounts, so the floor does not save their traversal.
        main.addSubview(UIView())
        let documents = UIView(frame: shell.bounds)
        documents.isUserInteractionEnabled = false
        shell.addSubview(documents)
        host.addChild(navigation)
        navigation.view.frame = main.bounds
        main.addSubview(navigation.view)
        navigation.didMove(toParent: host)
        window = UIWindow(windowScene: scene)
        window!.rootViewController = host
        window!.makeKeyAndVisible()
        driver.start(app: self)
    }

    @objc @inline(never) func push() {
        precondition(navigation.viewControllers.count == 1)
        if !PushProbe.shared.active { PushProbe.shared.begin() }
        let screen = Screen(station)
        navigation.setViewControllers([home, screen], animated: false)
        // Exact's apply performs a synchronous navigation layout before committing its explicit transaction.
        navigation.view.layoutIfNeeded()
    }

    func drive(_ command: String, reply: @escaping () -> Void) {
        switch command {
        case "push":
            PushProbe.shared.completed = reply
            PushProbe.shared.begin()
            change.sendActions(for: .touchUpInside)
        case "back":
            navigation.setViewControllers([home], animated: false)
            navigation.view.layoutIfNeeded()
            let observer = CFRunLoopObserverCreateWithHandler(nil, CFRunLoopActivity.beforeWaiting.rawValue, false, 2_000_001) { _, _ in reply() }
            CFRunLoopAddObserver(CFRunLoopGetMain(), observer, .commonModes)
        default: fatalError("unknown drive command")
        }
    }
}

final class Scene: UIResponder, UIWindowSceneDelegate {
    func scene(_ scene: UIScene, willConnectTo session: UISceneSession, options: UIScene.ConnectionOptions) {
        (UIApplication.shared.delegate as! App).setup(scene as! UIWindowScene)
    }
}

final class SocketDriver {
    func start(app: App) {
        guard let path = ProcessInfo.processInfo.environment["BAREREF_SOCKET"] else { return }
        DispatchQueue.global().async {
            let fd = socket(AF_UNIX, SOCK_STREAM, 0)
            var address = sockaddr_un()
            address.sun_family = sa_family_t(AF_UNIX)
            let bytes = path.utf8CString
            precondition(bytes.count <= MemoryLayout.size(ofValue: address.sun_path))
            withUnsafeMutableBytes(of: &address.sun_path) { $0.copyBytes(from: bytes.map { UInt8(bitPattern: $0) }) }
            let result = withUnsafePointer(to: &address) { pointer in
                pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                    bind(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
                }
            }
            precondition(result == 0 && listen(fd, 1) == 0)
            let peer = accept(fd, nil, nil)
            precondition(peer >= 0)
            defer { close(peer); close(fd) }
            var buffer = [UInt8](repeating: 0, count: 256), pending = ""
            while true {
                let n = read(peer, &buffer, buffer.count)
                if n <= 0 { return }
                pending += String(decoding: buffer.prefix(n), as: UTF8.self)
                while let end = pending.firstIndex(of: "\n") {
                    let command = String(pending[..<end])
                    pending.removeSubrange(...end)
                    DispatchQueue.main.async {
                        app.drive(command) {
                            let response = Array("ok\n".utf8)
                            _ = response.withUnsafeBytes { write(peer, $0.baseAddress!, $0.count) }
                        }
                    }
                }
            }
        }
    }
}
