import CoreGraphics
import Foundation

/// ImageIO reads the opened file, including its lazy reads, without reopening
/// a pathname that the app can replace. `pread` keeps concurrent decoders from
/// sharing a seek position. The provider owns its descriptor until released.
public enum FileImageSource {
    public static func provider(_ file: FileHandle) -> CGDataProvider? {
        var info = stat()
        guard fstat(file.fileDescriptor, &info) == 0, info.st_mode & S_IFMT == S_IFREG, info.st_size > 0 else { return nil }
        let fd = fcntl(file.fileDescriptor, F_DUPFD_CLOEXEC, 0)
        guard fd >= 0 else { return nil }
        let held = Unmanaged.passRetained(FileHandle(fileDescriptor: fd, closeOnDealloc: true))
        var callbacks = CGDataProviderDirectCallbacks(version: 0, getBytePointer: nil, releaseBytePointer: nil,
            getBytesAtPosition: { raw, buffer, offset, count in
                guard let raw else { return 0 }
                let file = Unmanaged<FileHandle>.fromOpaque(raw).takeUnretainedValue()
                var copied = 0
                while copied < count {
                    let n = pread(file.fileDescriptor, buffer.advanced(by: copied), count - copied, offset + Int64(copied))
                    if n < 0, errno == EINTR { continue }
                    guard n > 0 else { break }
                    copied += n
                }
                return copied
            }, releaseInfo: { raw in
                if let raw { Unmanaged<FileHandle>.fromOpaque(raw).release() }
            })
        guard let provider = CGDataProvider(directInfo: held.toOpaque(), size: info.st_size, callbacks: &callbacks) else {
            held.release()
            return nil
        }
        return provider
    }
}
