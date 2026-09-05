use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalSttEngineType {
    Parakeet,
    Moonshine,
    MoonshineStreaming,
    SenseVoice,
    GigaAm,
    Canary,
    Cohere,
}

#[derive(Clone, Debug, Serialize)]
pub struct LocalSttModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub filename: String,
    pub url: Option<String>,
    pub sha256: Option<String>,
    pub size_mb: u64,
    pub is_directory: bool,
    pub is_downloaded: bool,
    pub is_downloading: bool,
    pub partial_size: u64,
    pub engine_type: LocalSttEngineType,
    pub speed_score: f32,
    pub accuracy_score: f32,
    pub privacy_label: String,
    pub supported_languages: Vec<String>,
    pub supports_language_selection: bool,
    pub supports_translation: bool,
    pub is_recommended: bool,
}

#[derive(Clone, Debug)]
pub struct LocalSttModelManifest {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub filename: &'static str,
    pub url: Option<&'static str>,
    pub sha256: Option<&'static str>,
    pub size_mb: u64,
    pub is_directory: bool,
    pub install_name: &'static str,
    pub engine_type: LocalSttEngineType,
    pub speed_score: f32,
    pub accuracy_score: f32,
    pub privacy_label: &'static str,
    pub supported_languages: &'static [&'static str],
    pub supports_language_selection: bool,
    pub supports_translation: bool,
    pub is_recommended: bool,
}

impl LocalSttModelManifest {
    pub fn final_path(&self, root: &Path) -> PathBuf {
        root.join(self.install_name)
    }

    pub fn partial_download_path(&self, root: &Path) -> PathBuf {
        root.join(format!("{}.partial", self.filename))
    }

    pub fn extracting_path(&self, root: &Path) -> PathBuf {
        root.join(format!("{}.extracting", self.install_name))
    }

    pub fn is_downloaded(&self, root: &Path) -> bool {
        let path = self.final_path(root);
        if self.is_directory {
            path.is_dir()
        } else {
            path.is_file()
        }
    }

    pub fn partial_size(&self, root: &Path) -> u64 {
        std::fs::metadata(self.partial_download_path(root))
            .map(|meta| meta.len())
            .unwrap_or(0)
    }

    pub fn to_info(&self, root: &Path, is_downloading: bool) -> LocalSttModelInfo {
        LocalSttModelInfo {
            id: self.id.to_string(),
            name: self.name.to_string(),
            description: self.description.to_string(),
            filename: self.filename.to_string(),
            url: self.url.map(str::to_string),
            sha256: self.sha256.map(str::to_string),
            size_mb: self.size_mb,
            is_directory: self.is_directory,
            is_downloaded: self.is_downloaded(root),
            is_downloading,
            partial_size: self.partial_size(root),
            engine_type: self.engine_type,
            speed_score: self.speed_score,
            accuracy_score: self.accuracy_score,
            privacy_label: self.privacy_label.to_string(),
            supported_languages: self
                .supported_languages
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            supports_language_selection: self.supports_language_selection,
            supports_translation: self.supports_translation,
            is_recommended: self.is_recommended,
        }
    }
}

/// Built-in model catalog. URLs, checksums, and sizes are sourced from
/// Handy (https://github.com/cjpais/Handy) — the upstream project this
/// local STT feature is based on — and cross-verified against the actual
/// `blob.handy.computer` archives (HTTP Content-Length and tar listing both
/// matched Handy's published metadata exactly before these entries were
/// added). All of these load through the `onnx` Cargo feature already
/// enabled for Parakeet/Moonshine — no new build dependencies.
pub fn built_in_model_manifests() -> Vec<LocalSttModelManifest> {
    vec![
        LocalSttModelManifest {
            id: "parakeet-v3",
            name: "Parakeet V3",
            description: "Fast and accurate. Supports 25 European languages.",
            filename: "parakeet-v3-int8.tar.gz",
            url: Some("https://blob.handy.computer/parakeet-v3-int8.tar.gz"),
            sha256: Some("43d37191602727524a7d8c6da0eef11c4ba24320f5b4730f1a2497befc2efa77"),
            size_mb: 456,
            is_directory: true,
            install_name: "parakeet-tdt-0.6b-v3-int8",
            engine_type: LocalSttEngineType::Parakeet,
            speed_score: 4.25,
            accuracy_score: 4.0,
            privacy_label: "Runs on this device",
            supported_languages: &[
                "Bulgarian", "Croatian", "Czech", "Danish", "Dutch", "English", "Estonian",
                "Finnish", "French", "German", "Greek", "Hungarian", "Italian", "Latvian",
                "Lithuanian", "Maltese", "Polish", "Portuguese", "Romanian", "Slovak",
                "Slovenian", "Spanish", "Swedish", "Russian", "Ukrainian",
            ],
            supports_language_selection: false,
            supports_translation: false,
            is_recommended: true,
        },
        LocalSttModelManifest {
            id: "parakeet-v2",
            name: "Parakeet V2",
            description: "English-only alternative to Parakeet V3 with slightly higher English accuracy.",
            filename: "parakeet-v2-int8.tar.gz",
            url: Some("https://blob.handy.computer/parakeet-v2-int8.tar.gz"),
            sha256: Some("ac9b9429984dd565b25097337a887bb7f0f8ac393573661c651f0e7d31563991"),
            size_mb: 451,
            is_directory: true,
            install_name: "parakeet-tdt-0.6b-v2-int8",
            engine_type: LocalSttEngineType::Parakeet,
            speed_score: 4.25,
            accuracy_score: 4.25,
            privacy_label: "Runs on this device",
            supported_languages: &["English"],
            supports_language_selection: false,
            supports_translation: false,
            is_recommended: false,
        },
        LocalSttModelManifest {
            id: "moonshine-base",
            name: "Moonshine Base",
            description: "Smaller English model for weaker machines and faster local tests.",
            filename: "moonshine-base.tar.gz",
            url: Some("https://blob.handy.computer/moonshine-base.tar.gz"),
            sha256: Some("04bf6ab012cfceebd4ac7cf88c1b31d027bbdd3cd704649b692e2e935236b7e8"),
            size_mb: 187,
            is_directory: true,
            install_name: "moonshine-base",
            engine_type: LocalSttEngineType::Moonshine,
            speed_score: 4.8,
            accuracy_score: 3.3,
            privacy_label: "Runs on this device",
            supported_languages: &["English"],
            supports_language_selection: false,
            supports_translation: false,
            is_recommended: false,
        },
        LocalSttModelManifest {
            id: "moonshine-tiny",
            name: "Moonshine Tiny",
            description: "Smallest and fastest English model. Best for low-power machines.",
            filename: "moonshine-tiny-streaming-en.tar.gz",
            url: Some("https://blob.handy.computer/moonshine-tiny-streaming-en.tar.gz"),
            sha256: Some("465addcfca9e86117415677dfdc98b21edc53537210333a3ecdb58509a80abaf"),
            size_mb: 31,
            is_directory: true,
            install_name: "moonshine-tiny-streaming-en",
            engine_type: LocalSttEngineType::MoonshineStreaming,
            speed_score: 4.75,
            accuracy_score: 2.75,
            privacy_label: "Runs on this device",
            supported_languages: &["English"],
            supports_language_selection: false,
            supports_translation: false,
            is_recommended: false,
        },
        LocalSttModelManifest {
            id: "moonshine-small",
            name: "Moonshine Small",
            description: "Fast English model with a good balance of speed and accuracy.",
            filename: "moonshine-small-streaming-en.tar.gz",
            url: Some("https://blob.handy.computer/moonshine-small-streaming-en.tar.gz"),
            sha256: Some("dbb3e1c1832bd88a4ac712f7449a136cc2c9a18c5fe33a12ed1b7cb1cfe9cdd5"),
            size_mb: 99,
            is_directory: true,
            install_name: "moonshine-small-streaming-en",
            engine_type: LocalSttEngineType::MoonshineStreaming,
            speed_score: 4.5,
            accuracy_score: 3.25,
            privacy_label: "Runs on this device",
            supported_languages: &["English"],
            supports_language_selection: false,
            supports_translation: false,
            is_recommended: false,
        },
        LocalSttModelManifest {
            id: "moonshine-medium",
            name: "Moonshine Medium",
            description: "Higher quality English transcription, still fast.",
            filename: "moonshine-medium-streaming-en.tar.gz",
            url: Some("https://blob.handy.computer/moonshine-medium-streaming-en.tar.gz"),
            sha256: Some("07a66f3bff1c77e75a2f637e5a263928a08baae3c29c4c053fc968a9a9373d13"),
            size_mb: 192,
            is_directory: true,
            install_name: "moonshine-medium-streaming-en",
            engine_type: LocalSttEngineType::MoonshineStreaming,
            speed_score: 4.0,
            accuracy_score: 3.75,
            privacy_label: "Runs on this device",
            supported_languages: &["English"],
            supports_language_selection: false,
            supports_translation: false,
            is_recommended: false,
        },
        LocalSttModelManifest {
            id: "sense-voice",
            name: "SenseVoice",
            description: "Very fast multilingual model: Chinese, English, Japanese, Korean, Cantonese.",
            filename: "sense-voice-int8.tar.gz",
            url: Some("https://blob.handy.computer/sense-voice-int8.tar.gz"),
            sha256: Some("171d611fe5d353a50bbb741b6f3ef42559b1565685684e9aa888ef563ba3e8a4"),
            size_mb: 152,
            is_directory: true,
            install_name: "sense-voice-int8",
            engine_type: LocalSttEngineType::SenseVoice,
            speed_score: 4.75,
            accuracy_score: 3.25,
            privacy_label: "Runs on this device",
            supported_languages: &["Chinese", "English", "Japanese", "Korean", "Cantonese"],
            supports_language_selection: true,
            supports_translation: false,
            is_recommended: false,
        },
        LocalSttModelManifest {
            id: "gigaam-v3",
            name: "GigaAM v3",
            description: "Dedicated Russian speech recognition. Fast and accurate.",
            filename: "giga-am-v3-int8.tar.gz",
            url: Some("https://blob.handy.computer/giga-am-v3-int8.tar.gz"),
            sha256: Some("d872462268430db140b69b72e0fc4b787b194c1dbe51b58de39444d55b6da45b"),
            size_mb: 151,
            is_directory: true,
            install_name: "giga-am-v3-int8",
            engine_type: LocalSttEngineType::GigaAm,
            speed_score: 3.75,
            accuracy_score: 4.25,
            privacy_label: "Runs on this device",
            supported_languages: &["Russian"],
            supports_language_selection: false,
            supports_translation: false,
            is_recommended: false,
        },
        LocalSttModelManifest {
            id: "canary-180m-flash",
            name: "Canary 180M Flash",
            description: "Small, fast multilingual model: English, German, Spanish, French. Supports translation.",
            filename: "canary-180m-flash.tar.gz",
            url: Some("https://blob.handy.computer/canary-180m-flash.tar.gz"),
            sha256: Some("6d9cfca6118b296e196eaedc1c8fa9788305a7b0f1feafdb6dc91932ab6e53f7"),
            size_mb: 146,
            is_directory: true,
            install_name: "canary-180m-flash",
            engine_type: LocalSttEngineType::Canary,
            speed_score: 4.25,
            accuracy_score: 3.75,
            privacy_label: "Runs on this device",
            supported_languages: &["English", "German", "Spanish", "French"],
            supports_language_selection: true,
            supports_translation: true,
            is_recommended: false,
        },
        LocalSttModelManifest {
            id: "canary-1b-v2",
            name: "Canary 1B v2",
            description: "Larger, more accurate multilingual model. 25 European languages. Supports translation.",
            filename: "canary-1b-v2.tar.gz",
            url: Some("https://blob.handy.computer/canary-1b-v2.tar.gz"),
            sha256: Some("02305b2a25f9cf3e7deaffa7f94df00efa44f442cd55c101c2cb9c000f904666"),
            size_mb: 691,
            is_directory: true,
            install_name: "canary-1b-v2",
            engine_type: LocalSttEngineType::Canary,
            speed_score: 3.5,
            accuracy_score: 4.25,
            privacy_label: "Runs on this device",
            supported_languages: &[
                "Bulgarian", "Croatian", "Czech", "Danish", "Dutch", "English", "Estonian",
                "Finnish", "French", "German", "Greek", "Hungarian", "Italian", "Latvian",
                "Lithuanian", "Maltese", "Polish", "Portuguese", "Romanian", "Slovak",
                "Slovenian", "Spanish", "Swedish", "Russian", "Ukrainian",
            ],
            supports_language_selection: true,
            supports_translation: true,
            is_recommended: false,
        },
        LocalSttModelManifest {
            id: "cohere",
            name: "Cohere",
            description: "Largest and most accurate multilingual model. Covers European and East Asian languages, but slower.",
            filename: "cohere-int8.tar.gz",
            url: Some("https://blob.handy.computer/cohere-int8.tar.gz"),
            sha256: Some("ea2257d52434f3644574f187dcdcf666e302cd11b92866116ab8e14cd9c887f0"),
            size_mb: 1708,
            is_directory: true,
            install_name: "cohere-int8",
            engine_type: LocalSttEngineType::Cohere,
            speed_score: 3.0,
            accuracy_score: 4.5,
            privacy_label: "Runs on this device",
            supported_languages: &[
                "English", "French", "German", "Italian", "Spanish", "Portuguese", "Greek",
                "Dutch", "Polish", "Chinese", "Japanese", "Korean", "Vietnamese", "Arabic",
            ],
            supports_language_selection: true,
            supports_translation: false,
            is_recommended: false,
        },
    ]
}

pub fn manifest_by_id(model_id: &str) -> Option<LocalSttModelManifest> {
    built_in_model_manifests()
        .into_iter()
        .find(|manifest| manifest.id == model_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locks in the exact wire format of `LocalSttEngineType`'s serde
    /// `snake_case` serialization, since the frontend (`src/lib/tauri.ts`)
    /// hand-maintains a matching TypeScript union rather than generating it
    /// — a silent mismatch here would only surface at runtime as an
    /// unrecognized `engine_type` string in the UI.
    #[test]
    fn engine_type_serializes_to_expected_snake_case() {
        let cases = [
            (LocalSttEngineType::Parakeet, "\"parakeet\""),
            (LocalSttEngineType::Moonshine, "\"moonshine\""),
            (
                LocalSttEngineType::MoonshineStreaming,
                "\"moonshine_streaming\"",
            ),
            (LocalSttEngineType::SenseVoice, "\"sense_voice\""),
            (LocalSttEngineType::GigaAm, "\"giga_am\""),
            (LocalSttEngineType::Canary, "\"canary\""),
            (LocalSttEngineType::Cohere, "\"cohere\""),
        ];
        for (variant, expected) in cases {
            assert_eq!(serde_json::to_string(&variant).unwrap(), expected);
        }
    }

    #[test]
    fn every_manifest_has_a_unique_id_and_install_name() {
        let manifests = built_in_model_manifests();
        let mut ids = std::collections::HashSet::new();
        let mut install_names = std::collections::HashSet::new();
        for manifest in &manifests {
            assert!(ids.insert(manifest.id), "duplicate id: {}", manifest.id);
            assert!(
                install_names.insert(manifest.install_name),
                "duplicate install_name: {}",
                manifest.install_name
            );
        }
    }
}
