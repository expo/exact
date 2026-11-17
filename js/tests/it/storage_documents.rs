//! Documents and files a person chose, reached from TypeScript through the native storage provider.
#![cfg(exact_js_engine)]
use super::storage::{call, plan, Root, APP, GRANTS, HBC};
use exact_js::Module;
use exact_runner::{DataSource, Store};

/// A folder the person chose is reached from TypeScript as from Rust (LLP
/// 1069.010 D1, files F2): the same `doc:` path, the same grants, the same
/// executor, and with no app storage at all (a drive with no scratch store),
/// where an `app:/` read finds an empty store, as a fresh install's does.
#[test]
fn a_document_the_person_chose_is_storage_without_app_directories() {
    let dir = Root::new();
    let folder = dir.0.join("chosen");
    std::fs::create_dir_all(folder.join("sub")).unwrap();
    std::fs::write(folder.join("a.txt"), "A").unwrap();
    std::fs::write(folder.join("sub/b.txt"), "B").unwrap();
    let doc = exact_data::documents::mint(&folder, 9101).unwrap();
    let mut s = Store::new(GRANTS, Vec::<(String, Vec<u8>)>::new());
    let expected = "a.txt=A sub/0 new=hi Unavailable ENOENT Unavailable denied Unavailable EISDIR Unavailable ENOTDIR Unavailable ENOTEMPTY Unavailable failed Unavailable failed Unavailable ENOENT";
    let mut agent = Module::new(HBC.to_vec(), APP, GRANTS).with_agent_seed(Some(1));
    agent.set_document_owner(9101);
    agent.set_budget_ms(f64::INFINITY);
    agent.bind(&plan());
    agent.activate().unwrap();
    assert_eq!(
        call(&mut agent, &mut s, "doc", &doc),
        format!("{expected} Unavailable ENOENT")
    );
    let mut m = dir.module();
    m.set_document_owner(9101);
    m.activate().unwrap();
    assert_eq!(
        call(&mut m, &mut s, "doc", &doc),
        format!("{expected} Unavailable ENOENT")
    );
    assert!(!folder.join("new.txt").exists());
    let mut other = dir.module();
    other.set_document_owner(9103);
    other.activate().unwrap();
    assert_eq!(
        call(&mut other, &mut s, "doc-link", &format!("{doc}/a.txt")),
        ["refused"; 8].join(" ")
    );
    exact_data::documents::forget(9101);
}

#[test]
fn selected_file_directory_errors_reach_typescript_without_widening_access() {
    let root = Root::new();
    std::fs::create_dir_all(&root.0).unwrap();
    let file = root.0.join("chosen");
    std::fs::write(&file, b"original").unwrap();
    let doc = exact_data::documents::mint(&file, 9121).unwrap();
    let mut module = root.module();
    module.set_document_owner(9121);
    module.activate().unwrap();
    let mut store = Store::new(GRANTS, Vec::<(String, Vec<u8>)>::new());
    assert_eq!(
        call(&mut module, &mut store, "doc-file-codes", &doc),
        "Unavailable ENOTDIR Unavailable EEXIST Unavailable failed \
         Unavailable ENOTDIR Unavailable ENOTDIR Unavailable ENOTDIR"
    );
    assert_eq!(std::fs::read(&file).unwrap(), b"original");
    exact_data::documents::forget(9121);
}

#[test]
fn typescript_file_reads_obey_the_encoded_result_boundary() {
    let root = Root::new();
    let folder = root.0.join("chosen");
    std::fs::create_dir_all(&folder).unwrap();
    let file = folder.join("large");
    let boundary = ((exact_data::storage::MAX_BYTES - 13) / 4) * 3;
    std::fs::File::create(&file)
        .unwrap()
        .set_len(boundary as u64)
        .unwrap();
    let doc = exact_data::documents::mint(&file, 9120).unwrap();
    let mut module = root.module();
    module.set_document_owner(9120);
    module.activate().unwrap();
    let mut store = Store::new(GRANTS, Vec::<(String, Vec<u8>)>::new());
    assert_eq!(
        call(&mut module, &mut store, "read-size", &doc),
        boundary.to_string()
    );
    std::fs::OpenOptions::new()
        .write(true)
        .open(&file)
        .unwrap()
        .set_len((boundary + 1) as u64)
        .unwrap();
    assert!(call(&mut module, &mut store, "read-size", &doc)
        .contains("storage result exceeds its byte limit"));
    exact_data::documents::forget(9120);
}

#[test]
#[cfg(unix)]
fn typescript_document_operations_refuse_descendant_symlinks() {
    use std::os::unix::fs::symlink;
    let root = Root::new();
    let folder = root.0.join("chosen");
    let outside = root.0.join("outside");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("secret"), "outside").unwrap();
    symlink(outside.join("secret"), folder.join("link")).unwrap();
    symlink(&outside, folder.join("dir-link")).unwrap();
    let doc = exact_data::documents::mint(&folder, 9102).unwrap();
    let mut module = root.module();
    module.set_document_owner(9102);
    module.activate().unwrap();
    let mut store = Store::new(GRANTS, Vec::<(String, Vec<u8>)>::new());
    for leaf in ["link", "dir-link/secret"] {
        assert_eq!(
            call(
                &mut module,
                &mut store,
                "doc-link",
                &format!("{doc}/{leaf}")
            ),
            ["refused"; 8].join(" ")
        );
    }
    assert_eq!(
        std::fs::read_to_string(outside.join("secret")).unwrap(),
        "outside"
    );
    exact_data::documents::forget(9102);
}
