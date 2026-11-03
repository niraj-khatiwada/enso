use std::{
    env,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let icons_dir = manifest_dir.join("assets/icons");
    println!("cargo:rerun-if-changed=assets/icons");
    println!("cargo:rerun-if-changed=build.rs");

    let mut files: Vec<String> = fs::read_dir(&icons_dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", icons_dir.display()))
        .map(|entry| entry.expect("icon directory entry").file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".svg"))
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "no icons found in {}",
        icons_dir.display()
    );

    let mut out = String::from(
        "/// (file name, embedded SVG source) for every icon in `assets/icons/`.\n\
         pub(crate) static ICON_FILES: &[(&str, &str)] = &[\n",
    );
    for file in &files {
        writeln!(
            out,
            "    ({file:?}, include_str!({manifest:?})),",
            file = file,
            manifest = icons_dir.join(file).to_string_lossy(),
        )
        .expect("write generated icon table");
    }
    out.push_str("];\n");

    let dest = Path::new(&env::var("OUT_DIR").expect("OUT_DIR")).join("icons_generated.rs");
    fs::write(&dest, out).expect("write generated icon table");
}
