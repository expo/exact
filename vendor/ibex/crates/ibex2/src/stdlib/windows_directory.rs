// @ref LLP 0068#windows-app-storage-qualification — names stay under owned handles.
pub struct Kind {
    pub directory: bool,
    pub regular: bool,
}
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::mem::{offset_of, size_of};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::{Component, Path, Prefix};
use windows_sys::Wdk::Foundation::OBJECT_ATTRIBUTES;
use windows_sys::Wdk::Storage::FileSystem::*;
use windows_sys::Wdk::System::SystemServices::{FILE_FS_DEVICE_INFORMATION, FILE_REMOTE_DEVICE};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Storage::FileSystem::*;
use windows_sys::Win32::System::WindowsProgramming::{
    DRIVE_CDROM, DRIVE_FIXED, DRIVE_RAMDISK, DRIVE_REMOVABLE,
};
use windows_sys::Win32::System::IO::IO_STATUS_BLOCK;

pub fn refuse(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}
fn checked(status: NTSTATUS) -> io::Result<()> {
    if status < 0 {
        // SAFETY: this pure OS conversion accepts any NT status.
        Err(io::Error::from_raw_os_error(
            unsafe { RtlNtStatusToDosError(status) } as i32,
        ))
    } else {
        Ok(())
    }
}
fn name(value: &str) -> io::Result<Vec<u16>> {
    super::windows_path::validate_name(value)?;
    Ok(value.encode_utf16().collect())
}
pub fn validate_name(value: &str) -> io::Result<()> {
    name(value).map(|_| ())
}
fn validate(file: &File) -> io::Result<()> {
    if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(refuse(
            "filesystem entries cannot be symlinks or reparse points; root must be a real directory",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
enum Walk {
    App,
    Known,
}

fn open_known(file: File, access: u32) -> io::Result<File> {
    let info = attributes(&file)?;
    super::windows_path::open_known(file, info.FileAttributes, info.ReparseTag, |file| {
        reopen_known(file, access)
    })
}
fn attributes(file: &File) -> io::Result<FILE_ATTRIBUTE_TAG_INFO> {
    let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
    // SAFETY: the held handle and correctly sized output live through the query.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileAttributeTagInfo,
            (&mut info as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
            size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(info)
}
fn known_identity(file: &File) -> io::Result<(u64, [u8; 16])> {
    let mut info = FILE_ID_INFO::default();
    // SAFETY: a live handle and correctly sized output remain valid for the query.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            (&mut info as *mut FILE_ID_INFO).cast(),
            size_of::<FILE_ID_INFO>() as u32,
        )
    } != 0
    {
        let id = info.FileId.Identifier;
        if id == [0; 16] || id == [0xff; 16] {
            return Err(refuse("filesystem did not identify the placeholder"));
        }
        return Ok((info.VolumeSerialNumber, id));
    }
    let error = io::Error::last_os_error();
    if !matches!(
        error.raw_os_error().map(|code| code as u32),
        Some(ERROR_INVALID_PARAMETER | ERROR_INVALID_FUNCTION | ERROR_NOT_SUPPORTED)
    ) {
        return Err(error);
    }
    let (volume, id) = identity(file)?;
    if id == 0 || id == u64::MAX {
        return Err(refuse("filesystem did not identify the placeholder"));
    }
    let mut bytes = [0; 16];
    bytes[..8].copy_from_slice(&id.to_le_bytes());
    Ok((volume, bytes))
}
fn reopen_known(file: &File, access: u32) -> io::Result<File> {
    let before = known_identity(file)?;
    // Normal reparse processing lets the cloud filter recall the placeholder.
    // Reopen the held object, without OPEN_REPARSE_POINT or OPEN_NO_RECALL.
    // https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-reopenfile
    let handle = unsafe {
        ReOpenFile(
            file.as_raw_handle(),
            access | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            FILE_FLAG_BACKUP_SEMANTICS,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: ReOpenFile returned a new owned handle.
    let reopened = unsafe { File::from_raw_handle(handle) };
    let info = attributes(&reopened)?;
    if !super::windows_path::known_entry(info.FileAttributes, info.ReparseTag)
        || known_identity(&reopened)? != before
    {
        return Err(refuse("known-folder placeholder changed during recall"));
    }
    Ok(reopened)
}
fn open(
    parent: &Directory,
    leaf: &str,
    access: u32,
    disposition: u32,
    directory: bool,
) -> io::Result<File> {
    open_encoded(
        &parent.0,
        name(leaf)?,
        access,
        disposition,
        directory,
        parent.1,
        parent.2,
    )
}
fn open_encoded(
    parent: &File,
    mut encoded: Vec<u16>,
    access: u32,
    disposition: u32,
    directory: bool,
    insensitive: bool,
    walk: Walk,
) -> io::Result<File> {
    let mut unicode = UNICODE_STRING {
        Length: (encoded.len() * 2) as u16,
        MaximumLength: (encoded.len() * 2) as u16,
        Buffer: encoded.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &mut unicode,
        Attributes: if insensitive { OBJ_CASE_INSENSITIVE } else { 0 },
        ..Default::default()
    };
    let mut handle = std::ptr::null_mut();
    let mut status = IO_STATUS_BLOCK::default();
    // SAFETY: all pointers refer to live, correctly sized values; the single
    // component is resolved relative to the live parent, never a rebuilt path.
    checked(unsafe {
        NtCreateFile(
            &mut handle,
            access | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            &attributes,
            &mut status,
            std::ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            disposition,
            FILE_OPEN_REPARSE_POINT
                | FILE_SYNCHRONOUS_IO_NONALERT
                | if directory { FILE_DIRECTORY_FILE } else { 0 },
            std::ptr::null(),
            0,
        )
    })?;
    // SAFETY: a successful NtCreateFile transfers ownership of a new handle.
    let file = unsafe { File::from_raw_handle(handle) };
    match walk {
        Walk::App => {
            validate(&file)?;
            Ok(file)
        }
        Walk::Known => open_known(file, access),
    }
}
pub fn identity(file: &File) -> io::Result<(u64, u64)> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the handle is live and info has the required layout and size.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((
        u64::from(info.dwVolumeSerialNumber),
        u64::from(info.nFileIndexHigh) << 32 | u64::from(info.nFileIndexLow),
    ))
}
#[derive(Debug)]
pub struct Directory(pub File, bool, Walk);
impl Directory {
    /// The OS-selected root is trusted as named, including UNC and redirected
    /// roots. Below it, only name-surrogate reparse tags are refused (LLP 1173 D4).
    pub(crate) fn known(path: &Path, held: Option<File>) -> io::Result<Self> {
        let file = match held {
            Some(file) => file,
            None => OpenOptions::new()
                .access_mode(
                    FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
                )
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
                .open(path)?,
        };
        if !file.metadata()?.is_dir() {
            return Err(io::Error::new(io::ErrorKind::NotADirectory, "ENOTDIR"));
        }
        Ok(Self(file, true, Walk::Known))
    }
    pub(crate) fn selected(file: &File) -> io::Result<Self> {
        validate(file)?;
        if !file.metadata()?.is_dir() {
            return Err(refuse("document root must be a directory"));
        }
        Ok(Self(file.try_clone()?, true, Walk::App))
    }
    pub(crate) fn document_read(&self, leaf: &str) -> io::Result<Vec<u8>> {
        super::fs::read_result(self.document_file(leaf)?)
    }
    /// A chosen document's regular file, opened for reading.
    pub(crate) fn document_file(&self, leaf: &str) -> io::Result<File> {
        let file = open(self, leaf, FILE_READ_DATA, FILE_OPEN, false)?;
        let meta = file.metadata()?;
        if meta.is_dir() {
            return Err(io::Error::new(io::ErrorKind::IsADirectory, "EISDIR"));
        }
        if !meta.is_file() {
            return Err(refuse("document must be a regular file"));
        }
        Ok(file)
    }
    pub fn try_clone(&self) -> io::Result<Self> {
        Ok(Self(self.0.try_clone()?, self.1, self.2))
    }
    /// A native path operation pins its drive once. Case-sensitive directories
    /// keep their own lookup policy; the grant comparison is separately exact.
    pub fn native_drive(drive: u8) -> io::Result<Self> {
        let path = format!("{}:\\", drive as char);
        let name: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        // SAFETY: a terminated UTF-16 drive-root name lives through this call.
        let kind = unsafe { GetDriveTypeW(name.as_ptr()) };
        if !matches!(
            kind,
            DRIVE_FIXED | DRIVE_REMOVABLE | DRIVE_CDROM | DRIVE_RAMDISK
        ) {
            return Err(refuse("native filesystem requires a local drive"));
        }
        let mut root = Self::root(&path, false)?;
        root.1 = false;
        let mut info = FILE_FS_DEVICE_INFORMATION::default();
        let mut status = IO_STATUS_BLOCK::default();
        // SAFETY: the held directory and sized output structures remain live.
        checked(unsafe {
            NtQueryVolumeInformationFile(
                root.0.as_raw_handle(),
                &mut status,
                (&mut info as *mut FILE_FS_DEVICE_INFORMATION).cast(),
                size_of::<FILE_FS_DEVICE_INFORMATION>() as u32,
                FileFsDeviceInformation,
            )
        })?;
        if status.Information < size_of::<FILE_FS_DEVICE_INFORMATION>()
            || !matches!(info.DeviceType, FILE_DEVICE_DISK | FILE_DEVICE_CD_ROM)
            || info.Characteristics & FILE_REMOTE_DEVICE != 0
        {
            return Err(refuse(
                "opened native drive is not a qualified local filesystem",
            ));
        }
        Ok(root)
    }
    pub fn root(path: &str, create: bool) -> io::Result<Self> {
        let mut parts = Path::new(path).components();
        let drive = match parts.next() {
            Some(Component::Prefix(prefix)) => match prefix.kind() {
                Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => drive as char,
                _ => return Err(refuse("root requires an absolute local drive path")),
            },
            _ => return Err(refuse("root must be absolute")),
        };
        if parts.next() != Some(Component::RootDir) {
            return Err(refuse("root must be absolute"));
        }
        let file = OpenOptions::new()
            .access_mode(FILE_LIST_DIRECTORY | FILE_TRAVERSE | FILE_READ_ATTRIBUTES | SYNCHRONIZE)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(format!("{drive}:\\"))?;
        validate(&file)?;
        let mut dir = Self(file, true, Walk::App);
        for part in parts {
            match part {
                Component::Normal(part) => {
                    dir = dir.child(
                        part.to_str().ok_or_else(|| refuse("non-Unicode path"))?,
                        create,
                    )?
                }
                _ => return Err(refuse("invalid root component")),
            }
        }
        Ok(dir)
    }
    pub fn child(&self, leaf: &str, create: bool) -> io::Result<Self> {
        open(
            self,
            leaf,
            FILE_LIST_DIRECTORY | FILE_TRAVERSE,
            if create { FILE_OPEN_IF } else { FILE_OPEN },
            true,
        )
        .map(|file| Self(file, self.1, self.2))
    }
    pub(crate) fn create_child(&self, leaf: &str) -> io::Result<Self> {
        open(
            self,
            leaf,
            FILE_LIST_DIRECTORY | FILE_TRAVERSE,
            FILE_CREATE,
            true,
        )
        .map(|file| Self(file, self.1, self.2))
    }
    pub fn parent(&self, relative: &str, create: bool) -> io::Result<(Self, String)> {
        let parts: Vec<_> = relative.split('/').collect();
        for part in &parts {
            name(part)?;
        }
        let mut dir = self.try_clone()?;
        for part in &parts[..parts.len() - 1] {
            dir = dir.child(part, create)?;
        }
        Ok((dir, parts.last().unwrap().to_string()))
    }
    pub fn file(&self, leaf: &str, flags: i32) -> io::Result<File> {
        let mut access = 0;
        if flags & libc::O_WRONLY == 0 {
            access |= FILE_READ_DATA;
        }
        if flags & (libc::O_WRONLY | libc::O_RDWR) != 0 {
            access |= if flags & libc::O_APPEND != 0 {
                FILE_APPEND_DATA
            } else {
                FILE_WRITE_DATA
            };
        }
        let disposition = if flags & libc::O_EXCL != 0 {
            access |= DELETE;
            FILE_CREATE
        } else if flags & libc::O_CREAT != 0 {
            FILE_OPEN_IF
        } else {
            FILE_OPEN
        };
        let file = open(self, leaf, access, disposition, false)?;
        if !file.metadata()?.is_file() {
            return Err(refuse("filesystem entries must be regular files"));
        }
        Ok(file)
    }
    pub fn read(&self, leaf: &str) -> io::Result<Vec<u8>> {
        super::fs::read_result(self.file(leaf, libc::O_RDONLY)?)
    }
    pub fn entry(&self, leaf: &str) -> io::Result<File> {
        open(self, leaf, FILE_READ_ATTRIBUTES, FILE_OPEN, false)
    }
    pub fn removable(&self, leaf: &str) -> io::Result<File> {
        open(self, leaf, DELETE, FILE_OPEN, false)
    }
    pub fn kind(&self, leaf: &str) -> io::Result<Kind> {
        let file = open(self, leaf, FILE_READ_ATTRIBUTES, FILE_OPEN, false)?;
        let info = file.metadata()?;
        Ok(Kind {
            directory: info.is_dir(),
            regular: info.is_file(),
        })
    }
    pub fn names(&self) -> io::Result<Vec<String>> {
        self.names_limited(false)
    }
    pub fn names_batch(&self) -> io::Result<Vec<String>> {
        self.names_limited(true)
    }
    fn names_limited(&self, batch: bool) -> io::Result<Vec<String>> {
        // An empty NT relative name reopens the owned object, creating an
        // independent enumeration cursor without looking up its old pathname.
        let cursor = open_encoded(
            &self.0,
            Vec::new(),
            FILE_LIST_DIRECTORY,
            FILE_OPEN,
            true,
            self.1,
            self.2,
        )?;
        let mut storage = vec![0u64; 8192];
        let mut names = Vec::new();
        let mut used = 0;
        let mut class = FileIdBothDirectoryRestartInfo;
        loop {
            // SAFETY: storage is aligned and holds 65536 writable bytes.
            if unsafe {
                GetFileInformationByHandleEx(
                    cursor.as_raw_handle(),
                    class,
                    storage.as_mut_ptr().cast(),
                    (storage.len() * 8) as u32,
                )
            } == 0
            {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                    break;
                }
                return Err(error);
            }
            class = FileIdBothDirectoryInfo;
            let mut offset = 0;
            loop {
                let header = offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
                if offset + size_of::<FILE_ID_BOTH_DIR_INFO>() > storage.len() * 8 {
                    return Err(refuse("invalid directory entry"));
                }
                // SAFETY: bounded aligned offset supplied by the OS (checked below).
                let entry = unsafe {
                    &*storage
                        .as_ptr()
                        .cast::<u8>()
                        .add(offset)
                        .cast::<FILE_ID_BOTH_DIR_INFO>()
                };
                let length = entry.FileNameLength as usize;
                if !length.is_multiple_of(2) || offset + header + length > storage.len() * 8 {
                    return Err(refuse("invalid directory name"));
                }
                // SAFETY: the preceding check bounds this variable-length UTF-16 field.
                let units =
                    unsafe { std::slice::from_raw_parts(entry.FileName.as_ptr(), length / 2) };
                let leaf = String::from_utf16(units).map_err(|_| refuse("non-Unicode filename"))?;
                if leaf != "." && leaf != ".." {
                    name(&leaf)?;
                    if batch {
                        names.push(leaf);
                        if names.len() == 256 {
                            names.sort();
                            return Ok(names);
                        }
                    } else {
                        super::fs::push_name(&mut names, &mut used, leaf)?;
                    }
                }
                if entry.NextEntryOffset == 0 {
                    break;
                }
                let next = entry.NextEntryOffset as usize;
                if next < header + length || !next.is_multiple_of(8) {
                    return Err(refuse("invalid directory offset"));
                }
                offset += next;
            }
        }
        names.sort();
        Ok(names)
    }
    pub fn unlink(&self, leaf: &str) -> io::Result<()> {
        let file = open(self, leaf, DELETE, FILE_OPEN, false)?;
        let info = FILE_DISPOSITION_INFORMATION { DeleteFile: true };
        let mut status = IO_STATUS_BLOCK::default();
        // SAFETY: all structures are live and correctly sized; only this owned
        // file object is marked for deletion, with no further path resolution.
        checked(unsafe {
            NtSetInformationFile(
                file.as_raw_handle(),
                &mut status,
                (&info as *const FILE_DISPOSITION_INFORMATION).cast(),
                size_of::<FILE_DISPOSITION_INFORMATION>() as u32,
                FileDispositionInformation,
            )
        })
    }
    pub fn rename(&self, file: &File, leaf: &str) -> io::Result<()> {
        self.rename_using(file, leaf, true)
    }
    pub(crate) fn rename_new(&self, file: &File, leaf: &str) -> io::Result<()> {
        self.rename_using(file, leaf, false)
    }
    fn rename_using(&self, file: &File, leaf: &str, replace: bool) -> io::Result<()> {
        let leaf = name(leaf)?;
        let length = (offset_of!(FILE_RENAME_INFORMATION, FileName) + leaf.len() * 2)
            .max(size_of::<FILE_RENAME_INFORMATION>());
        let mut buffer = vec![0usize; length.div_ceil(size_of::<usize>())];
        // SAFETY: aligned allocation includes the complete variable name field.
        unsafe {
            let info = &mut *buffer.as_mut_ptr().cast::<FILE_RENAME_INFORMATION>();
            info.Anonymous.ReplaceIfExists = replace;
            info.RootDirectory = self.0.as_raw_handle();
            info.FileNameLength = (leaf.len() * 2) as u32;
            std::ptr::copy_nonoverlapping(leaf.as_ptr(), info.FileName.as_mut_ptr(), leaf.len());
        }
        let mut status = IO_STATUS_BLOCK::default();
        // SAFETY: file, parent, status, and variable-length information live
        // throughout the synchronous call; leaf is a validated single name.
        checked(unsafe {
            NtSetInformationFile(
                file.as_raw_handle(),
                &mut status,
                buffer.as_ptr().cast(),
                length as u32,
                FileRenameInformation,
            )
        })
    }
    pub fn write(&self, leaf: &str, bytes: &[u8], token: &str) -> io::Result<()> {
        self.write_checked(leaf, bytes, token, || Ok(()))
    }
    pub fn write_checked(
        &self,
        leaf: &str,
        bytes: &[u8],
        token: &str,
        before_commit: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<()> {
        let target = || match self.kind(leaf) {
            Ok(Kind { regular: true, .. }) => Ok(()),
            Ok(_) => Err(refuse("filesystem entries must be regular files")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        };
        target()?;
        if token.len() != 48 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(refuse("invalid temporary-file token"));
        }
        let temp = format!(".tmp-{}-{token}", std::process::id());
        let mut file = self.file(&temp, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL)?;
        let result = (|| {
            file.write_all(bytes)?;
            file.sync_all()?;
            before_commit()?;
            target()?;
            self.rename(&file, leaf)
        })();
        drop(file);
        let _ = self.unlink(&temp);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_reopens_the_held_directory_after_its_name_is_replaced() {
        let base = std::env::temp_dir().join(format!("ibex-known-recall-{}", std::process::id()));
        std::fs::create_dir_all(base.join("original")).unwrap();
        let root = Directory::known(&base, None).unwrap();
        let directory = root.child("original", false).unwrap();
        std::fs::rename(base.join("original"), base.join("held")).unwrap();
        std::fs::create_dir(base.join("original")).unwrap();
        let recalled = Directory(
            reopen_known(&directory.0, FILE_LIST_DIRECTORY | FILE_TRAVERSE).unwrap(),
            true,
            Walk::Known,
        );
        recalled
            .file("shot", libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL)
            .unwrap()
            .write_all(b"png")
            .unwrap();
        assert_eq!(std::fs::read(base.join("held/shot")).unwrap(), b"png");
        assert!(!base.join("original/shot").exists());
        drop(recalled);
        drop(directory);
        drop(root);
        std::fs::remove_dir_all(base).unwrap();
    }
}
