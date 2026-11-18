// What an embedder says of, and asks about, a session's native module. The
// words are kept without the module too: the orientation lock and the first
// screen read them (LLP 1047.001 D10; the module is `NativeModuleLink`'s).
public extension ExactSession {
    /// The embedder's word that this session's window is its own (LLP
    /// 1075.003.000.001 §2.1.1): a window hatch is then handed the window.
    /// Set before the session's first frame. False unless said.
    var hatchesOwnWindow: Bool {
        get { natives.ownsWindow }
        set { natives.ownsWindow = newValue }
    }
    /// The embedder's word that this session may set process-wide state
    /// (appearance proxies, the application object): at most one session in a
    /// process. False unless said.
    var hatchesOwnProcess: Bool {
        get { natives.ownsProcess }
        set { natives.ownsProcess = newValue }
    }
}

extension ExactSession {
    /// For a drive's ready line: why the app's module artifact did not load, or nil.
    public var nativeArtifactFailure: String? { natives.artifactFailure() }
}
