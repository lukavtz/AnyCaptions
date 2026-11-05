//! Runtime loading of the NeMo-Speech C libraries. The app builds and starts
//! without them; features report themselves unavailable when a library is missing.

use std::path::{Path, PathBuf};

use libloading::Library;

// Development build of NeMo-Speech.cpp (see backend/local-data/nemo-speech).
const DEV_NEMO_LIBRARY_DIR: &str = "../../backend/local-data/nemo-speech/build-vulkan-asr/bin";

/// Folder holding the NeMo-Speech C libraries: `ANYCAPTIONS_NEMO_DIR`, the local
/// development build (debug builds only), or the executable folder. Release builds
/// skip the development path so an installed app never reads from the source tree.
pub(crate) fn nemo_library_dir() -> PathBuf {
    let dev_dir = cfg!(debug_assertions)
        .then(|| Path::new(env!("CARGO_MANIFEST_DIR")).join(DEV_NEMO_LIBRARY_DIR));
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    [
        std::env::var_os("ANYCAPTIONS_NEMO_DIR").map(PathBuf::from),
        dev_dir,
        exe_dir.clone(),
    ]
    .into_iter()
    .flatten()
    .find(|dir| dir.is_dir())
    .unwrap_or_else(|| exe_dir.unwrap_or_else(|| PathBuf::from(".")))
}

#[cfg(windows)]
pub(crate) fn open_library(path: &Path) -> Result<Library, libloading::Error> {
    use libloading::os::windows::{LOAD_WITH_ALTERED_SEARCH_PATH, Library as WindowsLibrary};
    // The library's dependencies (ggml*.dll, llama.dll, nemo_speech_*.dll) live next to it,
    // not next to the executable; this flag makes Windows resolve them from the library's
    // own folder.
    // SAFETY: loading runs the library's initializers; it is the pinned NeMo-Speech.cpp build.
    unsafe { WindowsLibrary::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH).map(Into::into) }
}

#[cfg(not(windows))]
pub(crate) fn open_library(path: &Path) -> Result<Library, libloading::Error> {
    // SAFETY: loading runs the library's initializers; it is the pinned NeMo-Speech.cpp build.
    unsafe { Library::new(path) }
}
