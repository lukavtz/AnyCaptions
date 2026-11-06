//! Catalog and on-disk layout of the machine-learning models the app distributes.
//! Files live under one root, grouped by capability: `models/<kind>/<model>/<file>`.

pub mod catalog;
pub mod download;
mod verify;

use std::path::{Path, PathBuf};

pub use verify::ModelProfileError;

/// What a model does. Each kind owns a folder under the models root.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelKind {
    Asr,
    Translation,
    /// Speaker diarization: who spoke when, as generic speaker slots.
    Diarization,
}

impl ModelKind {
    pub const fn dir_name(self) -> &'static str {
        match self {
            Self::Asr => "asr",
            Self::Translation => "translation",
            Self::Diarization => "diarization",
        }
    }
}

/// Identity, provenance and integrity data for one distributed model file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelProfile {
    pub kind: ModelKind,
    pub id: &'static str,
    /// Folder under the kind directory; quantization variants of a model share it.
    pub directory: &'static str,
    pub filename: &'static str,
    pub revision: &'static str,
    pub source: &'static str,
    pub license: &'static str,
    pub sha256: &'static str,
    /// Exact artifact size when the publisher pins one; always enforced for ASR.
    pub size_bytes: Option<u64>,
    pub max_bytes: u64,
}

impl ModelProfile {
    /// Full path of the model file under `models_root`.
    pub fn path_in(&self, models_root: &Path) -> PathBuf {
        models_root
            .join(self.kind.dir_name())
            .join(self.directory)
            .join(self.filename)
    }

    /// Checks that the file at `path` exists, has the pinned size when known,
    /// stays within `max_bytes` and matches `sha256`.
    pub fn verify(&self, path: &Path) -> Result<(), ModelProfileError> {
        verify::verify_model_file(path, self.sha256, self.size_bytes, self.max_bytes)
    }
}

/// Models root: `ANYCAPTIONS_MODELS_DIR`, `backend/models` (debug builds only),
/// or `models/` next to the executable. Release builds skip the development path
/// so an installed app never reads models from the source tree.
pub fn models_root() -> PathBuf {
    let dev_models = cfg!(debug_assertions)
        .then(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../backend/models"));
    let exe_models = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("models")));
    [
        std::env::var_os("ANYCAPTIONS_MODELS_DIR").map(PathBuf::from),
        dev_models,
        exe_models.clone(),
    ]
    .into_iter()
    .flatten()
    .find(|dir| dir.is_dir())
    .unwrap_or_else(|| exe_models.unwrap_or_else(|| PathBuf::from("models")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_paths_are_grouped_by_kind_and_model_folder() {
        let path = catalog::NEMOTRON_STREAMING_Q8.path_in(Path::new("models"));
        assert_eq!(
            path,
            Path::new(
                "models/asr/nemotron-3.5-asr-streaming-0.6b/nemotron-3.5-asr-streaming-0.6b.q8_0.gguf"
            )
        );
    }

    #[test]
    fn catalog_profiles_carry_complete_integrity_data() {
        for profile in [
            &catalog::NEMOTRON_STREAMING_Q8,
            &catalog::NEMOTRON_STREAMING_EN_Q8,
            &catalog::NEMOTRON_3_DIARIZATION_Q8,
            &catalog::RIVA_TRANSLATE_Q4,
        ] {
            assert_eq!(profile.sha256.len(), 64);
            assert!(profile.sha256.bytes().all(|b| b.is_ascii_hexdigit()));
            assert!(profile.max_bytes > 0);
            assert!(profile.size_bytes.is_some_and(|bytes| bytes > 0));
            assert!(!profile.revision.is_empty());
        }
    }

    #[test]
    fn english_asr_profile_pins_the_candidate_artifact() {
        let profile = &catalog::NEMOTRON_STREAMING_EN_Q8;
        assert_eq!(profile.kind, ModelKind::Asr);
        assert_eq!(profile.revision, "ebe59e5a817142986528bbbee5dba8db7b38ed50");
        assert_eq!(profile.size_bytes, Some(699_872_960));
        assert_eq!(
            profile.sha256,
            "d9a01898d2a611c8764e23a1c2f45e70bbd5a425dc4de93692ac951dd603812d"
        );
        assert_eq!(profile.license, "NVIDIA Open Model License");
    }

    #[test]
    fn diarization_models_live_in_their_own_folder() {
        let path = catalog::NEMOTRON_3_DIARIZATION_Q8.path_in(Path::new("models"));
        assert_eq!(
            path,
            Path::new("models/diarization/nemotron-3-diarization/Nemotron-3-Diarization.q8_0.gguf")
        );
        assert_eq!(ModelKind::Diarization.dir_name(), "diarization");
    }
}
