use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn rc_string(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

fn file_version() -> Result<([u16; 4], String), Box<dyn std::error::Error>> {
    let package_version = env::var("CARGO_PKG_VERSION")?;
    let mut parts = package_version.split('.');
    let mut numeric = [0_u16; 4];
    for slot in &mut numeric[..3] {
        *slot = parts
            .next()
            .ok_or("CARGO_PKG_VERSION must have three numeric components")?
            .parse()?;
    }
    if parts.next().is_some() {
        return Err("CARGO_PKG_VERSION must have three numeric components".into());
    }
    Ok((numeric, package_version))
}

fn resource_source(icon: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let ([major, minor, patch, build], package_version) = file_version()?;
    let comma_version = format!("{major},{minor},{patch},{build}");
    Ok(format!(
        r#"#pragma code_page(65001)
1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {comma_version}
PRODUCTVERSION {comma_version}
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
      VALUE "CompanyName", "EduardoViniciusJ\0"
      VALUE "FileDescription", "AnyCaptions\0"
      VALUE "FileVersion", "{package_version}\0"
      VALUE "InternalName", "AnyCaptions\0"
      VALUE "OriginalFilename", "anycaptions.exe\0"
      VALUE "ProductName", "AnyCaptions\0"
      VALUE "ProductVersion", "{package_version}\0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        icon = rc_string(icon),
    ))
}

fn write_version_resource(out: &Path, icon: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let rc_path = out.join("anycaptions.rc");
    fs::write(&rc_path, resource_source(icon)?)?;
    Ok(rc_path)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-env-changed=RC");

    if env::var("CARGO_CFG_TARGET_OS")? != "windows" {
        return Ok(());
    }
    if env::var("CARGO_CFG_TARGET_ENV")? != "msvc" {
        return Err("AnyCaptions Windows resources require the MSVC target".into());
    }

    let crate_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").ok_or("CARGO_MANIFEST_DIR missing")?);
    let root = crate_dir.join("../..");
    let icon = root.join("backend/icons/icon.ico");
    let manifest = crate_dir.join("app.manifest");
    if !icon.is_file() {
        return Err(format!("application icon not found: {}", icon.display()).into());
    }
    println!("cargo:rerun-if-changed={}", icon.display());

    let out = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR missing")?);
    let rc_path = write_version_resource(&out, &icon)?;
    embed_resource::compile(rc_path, embed_resource::NONE).manifest_required()?;
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED,ID=1");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
    Ok(())
}
