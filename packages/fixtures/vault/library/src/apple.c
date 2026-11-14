#include <sys/mount.h>
#include <string.h>

// With locking styles disabled, SQLite 3.54.1 only reads f_fstypename to
// select its FAT/exFAT inode workaround. App-container storage needs neither.
// The renamed SDK declaration preserves Darwin's x86_64 INODE64 symbol label.
int exact_sqlite_fstatfs(int fd, struct statfs *info) {
    (void)fd;
    memset(info, 0, sizeof(*info));
    return 0;
}
