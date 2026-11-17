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

    /// A package can create a destination of `kind`. Check every existing
    /// component without following links; a missing suffix gives it nothing to
    /// follow.
    private static func destination(_ root: URL, _ names: [String], kind expected: mode_t) -> URL? {
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
                return kind == expected ? url : nil
            }
            let next = openat(current, name, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
            guard next >= 0 else { return nil }
            Darwin.close(current)
            current = next
        }
        return nil
    }

    /// A package can create or replace a regular file, but not a directory.
    public static func destination(_ root: URL, _ names: [String]) -> URL? {
        destination(root, names, kind: S_IFREG)
    }

    /// A package can create, fill or empty a directory, but not a regular file.
    public static func directory(_ root: URL, _ names: [String]) -> URL? {
        destination(root, names, kind: S_IFDIR)
    }

    /// Empty and recreate the last directory below `root`, then hold the new
    /// directory open. Every lookup and removal is relative to a checked
    /// descriptor, so replacing an admitted ancestor cannot redirect either
    /// the removal or a later `openat` by the caller.
    public static func replaceDirectory(_ root: URL, _ names: [String]) -> FileHandle? {
        guard valid(names) else { return nil }
        var current = Darwin.open(root.path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard current >= 0 else { return nil }
        defer { Darwin.close(current) }
        for name in names.dropLast() {
            var next = openat(current, name, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
            if next < 0, errno == ENOENT, mkdirat(current, name, 0o700) == 0 {
                next = openat(current, name, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
            }
            guard next >= 0 else { return nil }
            Darwin.close(current)
            current = next
        }
        guard let name = names.last else { return nil }
        var info = stat()
        if fstatat(current, name, &info, AT_SYMLINK_NOFOLLOW) == 0 {
            guard info.st_mode & S_IFMT == S_IFDIR,
                  let existing = openedDirectory(current, name), empty(existing.fileDescriptor) else { return nil }
            try? existing.close()
            guard unlinkat(current, name, AT_REMOVEDIR) == 0 else { return nil }
        } else if errno != ENOENT {
            return nil
        }
        guard mkdirat(current, name, 0o700) == 0,
              let made = openedDirectory(current, name) else { return nil }
        return made
    }

    private static func openedDirectory(_ parent: Int32, _ name: String) -> FileHandle? {
        let fd = openat(parent, name, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC)
        guard fd >= 0 else { return nil }
        return FileHandle(fileDescriptor: fd, closeOnDealloc: true)
    }

    private static func names(_ directory: Int32) -> [String]? {
        let copy = dup(directory)
        guard copy >= 0 else { return nil }
        guard let stream = fdopendir(copy) else { Darwin.close(copy); return nil }
        defer { closedir(stream) }
        var result: [String] = []
        while let entry = readdir(stream) {
            let name = withUnsafePointer(to: &entry.pointee.d_name) {
                $0.withMemoryRebound(to: CChar.self, capacity: Int(MAXPATHLEN)) { String(cString: $0) }
            }
            if name != ".", name != ".." { result.append(name) }
        }
        return result
    }

    private static func empty(_ directory: Int32) -> Bool {
        guard let children = names(directory) else { return false }
        for name in children {
            var info = stat()
            guard fstatat(directory, name, &info, AT_SYMLINK_NOFOLLOW) == 0 else { return false }
            if info.st_mode & S_IFMT == S_IFDIR {
                guard let child = openedDirectory(directory, name), empty(child.fileDescriptor) else { return false }
                try? child.close()
                guard unlinkat(directory, name, AT_REMOVEDIR) == 0 else { return false }
            } else if unlinkat(directory, name, 0) != 0 {
                return false
            }
        }
        return true
    }
}
