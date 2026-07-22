use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let project_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("propagation crate must live under <project>/crates");
    let itm_root = project_root.join("third_party/ntia-itm");
    let source_dir = itm_root.join("src");
    let include_dir = itm_root.join("include");
    let wrapper = manifest_dir.join("native/itm_wrapper.cpp");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let portable_source_dir = out_dir.join("ntia-itm-portable-src");

    assert!(source_dir.is_dir(), "missing vendored NTIA ITM source");
    fs::create_dir_all(&portable_source_dir).expect("create portable ITM source directory");

    let mut upstream_sources: Vec<PathBuf> = fs::read_dir(&source_dir)
        .expect("read NTIA ITM source directory")
        .map(|entry| entry.expect("read NTIA ITM directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "cpp"))
        .collect();
    upstream_sources.sort();

    // NTIA v1.4 uses Windows backslashes in every source-file include. Create
    // a build-only portable mirror, preserving the vendored source byte-for-byte.
    let sources: Vec<PathBuf> = upstream_sources
        .iter()
        .map(|source| {
            let contents = fs::read_to_string(source).expect("read NTIA ITM source file");
            let portable = contents.replace("..\\include\\", "");
            let destination = portable_source_dir.join(source.file_name().unwrap());
            fs::write(&destination, portable).expect("write portable ITM source file");
            destination
        })
        .collect();

    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let mut build = cc::Build::new();
    build
        .cpp(true)
        .include(&include_dir)
        .files(&sources)
        .file(&wrapper)
        .warnings(false);

    if target_env == "msvc" {
        build.flag_if_supported("/std:c++17");
        build.flag_if_supported("/O2");
    } else {
        build.std("c++17");
        build.flag_if_supported("-O3");
        build.flag_if_supported("-fPIC");
        // NTIA v1.4 declares Windows DLL exports unconditionally. On Unix the
        // annotation must expand to nothing; the implementation remains intact.
        build.define("__declspec(x)", Some(""));
    }

    build.compile("hamheatmap_itm");

    println!("cargo:rerun-if-changed={}", wrapper.display());
    println!(
        "cargo:rerun-if-changed={}",
        manifest_dir.join("native/itm_wrapper.h").display()
    );
    println!("cargo:rerun-if-changed={}", include_dir.display());
    println!("cargo:rerun-if-changed={}", source_dir.display());
}
