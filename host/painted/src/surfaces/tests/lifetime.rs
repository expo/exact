use super::*;
use std::io::{Read as _, Write as _};
use std::os::{fd::AsRawFd, unix::net::UnixStream};
use std::sync::mpsc;
use std::time::Duration;

fn event(stream: &mut UnixStream) -> u8 {
    let mut byte = [0];
    stream.read_exact(&mut byte).unwrap();
    byte[0]
}

#[test]
fn primary_and_secondary_libraries_stay_loaded_until_unload_finishes() {
    for artifact in ["", "secondary"] {
        let (path, mut compat) = fixture();
        let source = path.with_file_name("probe.c");
        let text = std::fs::read_to_string(&source)
            .unwrap()
            .replace("void gpu_unload(void) {}", "void gpu_unload(void);");
        // The worker executes in the artifact. Its destructor reports the actual
        // dlclose, without another Library handle keeping the code resident.
        std::fs::write(
            &source,
            format!(
                r#"{text}
#include <pthread.h>
#include <unistd.h>
static int events = -1, permit;
static pthread_t worker;
static void report(char event) {{ write(events, &event, 1); }}
static void *acquire(void *unused) {{
    report('w');
    char byte;
    read(permit, &byte, 1);
    report('e');
    return 0;
}}
uint32_t test_start(int events_fd, int permit_fd) {{
    events = events_fd; permit = permit_fd;
    return pthread_create(&worker, 0, acquire, 0);
}}
void gpu_unload(void) {{
    report('u');
    pthread_join(worker, 0);
    report('j');
}}
__attribute__((destructor)) static void closed(void) {{
    if (events >= 0) report('c');
}}
"#
            ),
        )
        .unwrap();
        compile_fixture(&source, &path);
        compat["embedded"]["gpu"]["sha256"] = digest::file(&path, true).unwrap().into();
        if !artifact.is_empty() {
            compat["embedded"]["gpuModules"][artifact] = compat["embedded"]["gpu"].take();
        }
        let (mut events, reports) = UnixStream::pair().unwrap();
        let (mut release, permit) = UnixStream::pair().unwrap();
        events
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let (shutdown, shutting_down) = mpsc::channel();
        let file = path.clone();
        let owner = std::thread::spawn(move || {
            let abi = Abi::open_path(&file, &compat, artifact).unwrap();
            // SAFETY: fixture export; both descriptors live past the worker and
            // the library's destructor. No second library handle is retained.
            assert_eq!(
                unsafe {
                    abi.symbol::<unsafe extern "C" fn(i32, i32) -> u32>(b"test_start")(
                        reports.as_raw_fd(),
                        permit.as_raw_fd(),
                    )
                },
                0
            );
            let _ = shutting_down.recv();
            drop(abi);
        });
        assert_eq!(event(&mut events), b'w', "{artifact}: acquisition entered");
        shutdown.send(()).unwrap();
        assert_eq!(event(&mut events), b'u', "{artifact}: unload entered");
        release.write_all(&[1]).unwrap();
        owner.join().unwrap();
        let mut rest = Vec::new();
        events.read_to_end(&mut rest).unwrap();
        assert_eq!(
            rest, b"ejc",
            "{artifact}: worker exits, unload joins, dlclose"
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
