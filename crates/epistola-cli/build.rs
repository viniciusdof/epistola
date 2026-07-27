#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "build scripts are exempt"
)]

fn main() {
    #[cfg_attr(not(windows), allow(unused_variables))]
    let build = emit_build_info_env();
    #[cfg(windows)]
    embed_icon(&build);
}

include!("../../build-support/build_info.rs");

#[cfg(windows)]
fn embed_icon(build: &BuildEnv) {
    println!("cargo:rerun-if-changed=../../resources/icon.ico");

    let icon =
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../resources/icon.ico").replace('\\', "\\\\");
    let pkg_version = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    let product_version = if build.channel == "nightly" {
        let dirty = if build.dirty { "-dirty" } else { "" };
        format!("nightly {} ({}{dirty})", build.git_date, build.git_sha)
    } else {
        pkg_version.clone()
    };
    let mut version_parts = pkg_version
        .split('.')
        .map(|part| part.parse::<u16>().unwrap_or(0))
        .chain(std::iter::repeat(0));
    let file_version = format!(
        "{},{},{},{}",
        version_parts.next().unwrap_or(0),
        version_parts.next().unwrap_or(0),
        version_parts.next().unwrap_or(0),
        version_parts.next().unwrap_or(0),
    );

    let rc_content = format!(
        r#"1 ICON "{icon}"

1 VERSIONINFO
FILEVERSION {file_version}
PRODUCTVERSION {file_version}
FILEFLAGSMASK 0x3fL
FILEFLAGS 0x0L
FILEOS 0x40004L
FILETYPE 0x1L
FILESUBTYPE 0x0L
BEGIN
    BLOCK "StringFileInfo"
    BEGIN
        BLOCK "040904b0"
        BEGIN
            VALUE "FileDescription", "Epistola CLI\0"
            VALUE "FileVersion", "{pkg_version}\0"
            VALUE "ProductName", "Epistola\0"
            VALUE "ProductVersion", "{product_version}\0"
        END
    END
    BLOCK "VarFileInfo"
    BEGIN
        VALUE "Translation", 0x0409, 1200
    END
END
"#
    );

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let rc_path = std::path::Path::new(&out_dir).join("epistola_cli_resources.rc");
    std::fs::write(&rc_path, rc_content).expect("failed to write resource script");

    embed_resource::compile(&rc_path, embed_resource::NONE)
        .manifest_optional()
        .expect("failed to compile Windows resources");
}
