// How an Apple reader opens a file the app names under `app:/` (@ref LLP
// 1069.002 D7). ExactCore and the app's module (host/apple/compose.mjs
// `moduleSeam`, for `ExactExtensions.file`) each compile this file into their
// own module.
import Foundation

public enum AppFileWalk {
    private static func valid(_ names: [String]) -> Bool {
        !names.isEmpty && !names.contains(where: { $0.isEmpty || $0 == "." || $0 == ".." || $0.contains("/") || $0.contains("\0") })
    }

    /// The regular file at `names` below the directory `root`, opened as the
    /// data module's `readFile` opens one (ibex2's `app_fs_unix.rs`): every
    /// part, the root's last included, with `O_NOFOLLOW`, so a symbolic link
    /// the app's own code made in its directories leads nowhere. nil when a
    /// part is missing, empty, `.`, `..`, a link or not a directory, or the
    /// last is not a regular file.
    public static func open(_ root: URL, _ names: [String]) -> FileHandle? {
        guard valid(names) else { return nil }
        var current = Darwin.open(root.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        for (index, name) in names.enumerated() where current >= 0 {
            let last = index == names.count - 1
            // A FIFO's open would wait for a writer: the last part opens without blocking.
            let next = openat(current, name, (last ? O_RDONLY | O_NONBLOCK : O_RDONLY | O_DIRECTORY) | O_NOFOLLOW | O_CLOEXEC)
            Darwin.close(current)
            current = next
        }
        var info = stat()
        guard current >= 0 else { return nil }
        guard fstat(current, &info) == 0, info.st_mode & S_IFMT == S_IFREG else {
            Darwin.close(current)
            return nil
        }
        return FileHandle(fileDescriptor: current, closeOnDealloc: true)
    }

    /// An app path in the asking session's roots, never another session's.
    public static func open(_ path: String, roots: [String: URL]) -> FileHandle? {
        guard path.hasPrefix("app:/") else { return nil }
        let parts = path.dropFirst(5).split(separator: "/", omittingEmptySubsequences: false).map(String.init)
        guard let name = parts.first, let root = roots[name] else { return nil }
        return open(root, Array(parts.dropFirst()))
    }

    /// A package can create its file destination. Check every existing component
    /// without following links; a missing suffix gives it nothing to follow.
    public static func destination(_ root: URL, _ names: [String]) -> URL? {
        guard valid(names) else { return nil }
        var current = Darwin.open(root.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard current >= 0 else { return nil }
        defer { Darwin.close(current) }
        let url = names.reduce(root) { $0.appendingPathComponent($1) }
        for (index, name) in names.enumerated() {
            var info = stat()
            if fstatat(current, name, &info, AT_SYMLINK_NOFOLLOW) != 0 {
                return errno == ENOENT ? url : nil
            }
            let kind = info.st_mode & S_IFMT
            if index == names.count - 1 {
                return kind == S_IFREG ? url : nil
            }
            let next = openat(current, name, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
            guard next >= 0 else { return nil }
            Darwin.close(current)
            current = next
        }
        return nil
    }
}
