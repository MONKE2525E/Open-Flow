use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalLlmPromptFamily {
    Gemma4,
    Qwen25,
    Phi3,
    Smollm2,
    Granite33,
}

#[derive(Clone, Debug, Serialize)]
pub struct LocalLlmModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub repo_id: String,
    pub size_mb: u64,
    pub quantization: String,
    pub privacy_label: String,
    pub is_downloaded: bool,
    pub is_downloading: bool,
    pub partial_size: u64,
    pub is_recommended: bool,
    pub prompt_family: LocalLlmPromptFamily,
}

#[derive(Clone, Debug)]
pub struct LocalLlmArtifact {
    pub filename: &'static str,
    pub sha256: Option<&'static str>,
}

#[derive(Clone, Debug)]
pub struct LocalLlmModelManifest {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub repo_id: &'static str,
    pub size_mb: u64,
    pub quantization: &'static str,
    pub privacy_label: &'static str,
    pub artifacts: &'static [LocalLlmArtifact],
    pub primary_artifact: &'static str,
    pub is_recommended: bool,
    pub prompt_family: LocalLlmPromptFamily,
}

impl LocalLlmModelManifest {
    pub fn final_path(&self, root: &Path) -> PathBuf {
        root.join(self.id)
    }

    pub fn partial_download_path(&self, root: &Path) -> PathBuf {
        root.join(format!("{}.partial", self.id))
    }

    pub fn is_downloaded(&self, root: &Path) -> bool {
        let dir = self.final_path(root);
        dir.is_dir()
            && self
                .artifacts
                .iter()
                .all(|artifact| dir.join(artifact.filename).is_file())
    }

    pub fn partial_size(&self, root: &Path) -> u64 {
        let dir = self.partial_download_path(root);
        if !dir.is_dir() {
            return 0;
        }
        self.artifacts
            .iter()
            .map(|artifact| {
                std::fs::metadata(dir.join(format!("{}.partial", artifact.filename)))
                    .map(|meta| meta.len())
                    .unwrap_or(0)
            })
            .sum()
    }

    pub fn primary_model_path(&self, root: &Path) -> PathBuf {
        self.final_path(root).join(self.primary_artifact)
    }

    pub fn to_info(&self, root: &Path, is_downloading: bool) -> LocalLlmModelInfo {
        LocalLlmModelInfo {
            id: self.id.to_string(),
            name: self.name.to_string(),
            description: self.description.to_string(),
            repo_id: self.repo_id.to_string(),
            size_mb: self.size_mb,
            quantization: self.quantization.to_string(),
            privacy_label: self.privacy_label.to_string(),
            is_downloaded: self.is_downloaded(root),
            is_downloading,
            partial_size: self.partial_size(root),
            is_recommended: self.is_recommended,
            prompt_family: self.prompt_family,
        }
    }
}

// SHA256 values below are the git-lfs OIDs published by each model's
// Hugging Face repo (fetched from https://huggingface.co/api/models/{repo}/tree/main,
// where the LFS `oid` field is the file's SHA256 content hash) — not
// computed locally, since that would require downloading every multi-GB
// file just to populate this table.
const GEMMA_E2B_ARTIFACTS: &[LocalLlmArtifact] = &[
    LocalLlmArtifact {
        filename: "gemma-4-E2B_q4_0-it.gguf",
        sha256: Some("3646b4c147cd235a44d91df1546d3b7d8e29b547dbe4e1f80856419aa455e6fd"),
    },
    LocalLlmArtifact {
        filename: "gemma-4-E2B-it-mmproj.gguf",
        sha256: Some("58c187648007cab392bd5678b87e862c3e8794017deb945feea2cf256195e96a"),
    },
];

const GEMMA_E4B_ARTIFACTS: &[LocalLlmArtifact] = &[
    LocalLlmArtifact {
        filename: "gemma-4-E4B_q4_0-it.gguf",
        sha256: Some("e8b6a059ba86947a44ace84d6e5679795bc41862c25c30513142588f0e9dba1d"),
    },
    LocalLlmArtifact {
        filename: "gemma-4-E4B-it-mmproj.gguf",
        sha256: Some("c6398448d84a4836fdedf58f9775979e69ae0cc4dfdf4d697b5597693a555b12"),
    },
];

const QWEN_05B_ARTIFACTS: &[LocalLlmArtifact] = &[LocalLlmArtifact {
    filename: "qwen2.5-0.5b-instruct-q4_k_m.gguf",
    sha256: Some("74a4da8c9fdbcd15bd1f6d01d621410d31c6fc00986f5eb687824e7b93d7a9db"),
}];

const QWEN_15B_ARTIFACTS: &[LocalLlmArtifact] = &[LocalLlmArtifact {
    filename: "qwen2.5-1.5b-instruct-q4_k_m.gguf",
    sha256: Some("6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e"),
}];

const QWEN_3B_ARTIFACTS: &[LocalLlmArtifact] = &[LocalLlmArtifact {
    filename: "qwen2.5-3b-instruct-q4_k_m.gguf",
    sha256: Some("626b4a6678b86442240e33df819e00132d3ba7dddfe1cdc4fbb18e0a9615c62d"),
}];

const QWEN_7B_ARTIFACTS: &[LocalLlmArtifact] = &[
    LocalLlmArtifact {
        filename: "qwen2.5-7b-instruct-q4_k_m-00001-of-00002.gguf",
        sha256: Some("dfce12e3862a5283ccfb88221b48480e58745165de856439950d0f22590580db"),
    },
    LocalLlmArtifact {
        filename: "qwen2.5-7b-instruct-q4_k_m-00002-of-00002.gguf",
        sha256: Some("539cf93f78e887edea1c04e2d7d8cdaca9d01dae9c9025bcb8accbe29df3d72a"),
    },
];

const PHI3_ARTIFACTS: &[LocalLlmArtifact] = &[LocalLlmArtifact {
    filename: "Phi-3-mini-4k-instruct-q4.gguf",
    sha256: Some("8a83c7fb9049a9b2e92266fa7ad04933bb53aa1e85136b7b30f1b8000ff2edef"),
}];

const SMOLLM2_360M_ARTIFACTS: &[LocalLlmArtifact] = &[LocalLlmArtifact {
    filename: "smollm2-360m-instruct-q8_0.gguf",
    sha256: Some("48ab3034d0dd401fbc721eb1df3217902fee7dab9078992d66431f09b7750201"),
}];

const SMOLLM2_17B_ARTIFACTS: &[LocalLlmArtifact] = &[LocalLlmArtifact {
    filename: "smollm2-1.7b-instruct-q4_k_m.gguf",
    sha256: Some("decd2598bc2c8ed08c19adc3c8fdd461ee19ed5708679d1c54ef54a5a30d4f33"),
}];

const GRANITE_2B_ARTIFACTS: &[LocalLlmArtifact] = &[LocalLlmArtifact {
    filename: "granite-3.3-2b-instruct-Q4_K_M.gguf",
    sha256: Some("ac71e9e32c0bea919b409c5918f69ca74339854b0319c5065e4e9fb6d95c4852"),
}];

const GRANITE_8B_ARTIFACTS: &[LocalLlmArtifact] = &[LocalLlmArtifact {
    filename: "granite-3.3-8b-instruct-Q4_K_M.gguf",
    sha256: Some("77bcee066a76dcdd10d0d123c87e32c8ec2c74e31b6ffd87ebee49c9ac215dca"),
}];

pub fn built_in_model_manifests() -> Vec<LocalLlmModelManifest> {
    vec![
        LocalLlmModelManifest {
            id: "qwen2.5-1.5b-instruct",
            name: "Qwen 2.5 1.5B Instruct",
            description: "Best small default for local cleanup. Reliable ChatML format, fast, low memory footprint.",
            repo_id: "Qwen/Qwen2.5-1.5B-Instruct-GGUF",
            size_mb: 1080,
            quantization: "Q4_K_M",
            privacy_label: "Runs on this device",
            artifacts: QWEN_15B_ARTIFACTS,
            primary_artifact: "qwen2.5-1.5b-instruct-q4_k_m.gguf",
            is_recommended: true,
            prompt_family: LocalLlmPromptFamily::Qwen25,
        },
        LocalLlmModelManifest {
            id: "qwen2.5-3b-instruct",
            name: "Qwen 2.5 3B Instruct",
            description: "Balanced local cleanup model with strong formatting control and good latency — a step up from the 1.5B default when you want more quality.",
            repo_id: "Qwen/Qwen2.5-3B-Instruct-GGUF",
            size_mb: 1960,
            quantization: "Q4_K_M",
            privacy_label: "Runs on this device",
            artifacts: QWEN_3B_ARTIFACTS,
            primary_artifact: "qwen2.5-3b-instruct-q4_k_m.gguf",
            is_recommended: true,
            prompt_family: LocalLlmPromptFamily::Qwen25,
        },
        LocalLlmModelManifest {
            id: "phi-3-mini-4k-instruct",
            name: "Phi-3 Mini 4K Instruct",
            description: "Compact Microsoft model with good cleanup reliability and a short context window.",
            repo_id: "microsoft/Phi-3-mini-4k-instruct-gguf",
            size_mb: 2280,
            quantization: "Q4",
            privacy_label: "Runs on this device",
            artifacts: PHI3_ARTIFACTS,
            primary_artifact: "Phi-3-mini-4k-instruct-q4.gguf",
            is_recommended: true,
            prompt_family: LocalLlmPromptFamily::Phi3,
        },
        LocalLlmModelManifest {
            id: "gemma-4-e2b",
            name: "Gemma 4 E2B",
            description: "Not recommended for now — this curated GGUF conversion has shown reliability issues in testing (malformed tokenizer metadata, inconsistent output length). Try a Qwen or Phi-3 model above instead.",
            repo_id: "google/gemma-4-E2B-it-qat-q4_0-gguf",
            size_mb: 1640,
            quantization: "Q4_0",
            privacy_label: "Runs on this device",
            artifacts: GEMMA_E2B_ARTIFACTS,
            primary_artifact: "gemma-4-E2B_q4_0-it.gguf",
            is_recommended: false,
            prompt_family: LocalLlmPromptFamily::Gemma4,
        },
        LocalLlmModelManifest {
            id: "gemma-4-e4b",
            name: "Gemma 4 E4B",
            description: "Not recommended for now — same tokenizer metadata issues as Gemma 4 E2B, just at a larger size. Try a Qwen or Phi-3 model above instead.",
            repo_id: "google/gemma-4-E4B-it-qat-q4_0-gguf",
            size_mb: 3260,
            quantization: "Q4_0",
            privacy_label: "Runs on this device",
            artifacts: GEMMA_E4B_ARTIFACTS,
            primary_artifact: "gemma-4-E4B_q4_0-it.gguf",
            is_recommended: false,
            prompt_family: LocalLlmPromptFamily::Gemma4,
        },
        LocalLlmModelManifest {
            id: "qwen2.5-0.5b-instruct",
            name: "Qwen 2.5 0.5B Instruct",
            description: "Tiny fallback for weak machines. Faster, but needs stricter cleanup prompting.",
            repo_id: "Qwen/Qwen2.5-0.5B-Instruct-GGUF",
            size_mb: 430,
            quantization: "Q4_K_M",
            privacy_label: "Runs on this device",
            artifacts: QWEN_05B_ARTIFACTS,
            primary_artifact: "qwen2.5-0.5b-instruct-q4_k_m.gguf",
            is_recommended: false,
            prompt_family: LocalLlmPromptFamily::Qwen25,
        },
        LocalLlmModelManifest {
            id: "qwen2.5-7b-instruct",
            name: "Qwen 2.5 7B Instruct",
            description: "Largest Qwen pick in the curated catalog. Good quality, much heavier download.",
            repo_id: "Qwen/Qwen2.5-7B-Instruct-GGUF",
            size_mb: 4680,
            quantization: "Q4_K_M",
            privacy_label: "Runs on this device",
            artifacts: QWEN_7B_ARTIFACTS,
            primary_artifact: "qwen2.5-7b-instruct-q4_k_m-00001-of-00002.gguf",
            is_recommended: false,
            prompt_family: LocalLlmPromptFamily::Qwen25,
        },
        LocalLlmModelManifest {
            id: "smollm2-360m-instruct",
            name: "SmolLM2 360M Instruct",
            description: "Extreme low-end option. Official repo only ships Q8, so it stays an advanced pick.",
            repo_id: "HuggingFaceTB/SmolLM2-360M-Instruct-GGUF",
            size_mb: 390,
            quantization: "Q8_0",
            privacy_label: "Runs on this device",
            artifacts: SMOLLM2_360M_ARTIFACTS,
            primary_artifact: "smollm2-360m-instruct-q8_0.gguf",
            is_recommended: false,
            prompt_family: LocalLlmPromptFamily::Smollm2,
        },
        LocalLlmModelManifest {
            id: "smollm2-1.7b-instruct",
            name: "SmolLM2 1.7B Instruct",
            description: "Sharper than the 360M model while still staying relatively light.",
            repo_id: "HuggingFaceTB/SmolLM2-1.7B-Instruct-GGUF",
            size_mb: 1030,
            quantization: "Q4_K_M",
            privacy_label: "Runs on this device",
            artifacts: SMOLLM2_17B_ARTIFACTS,
            primary_artifact: "smollm2-1.7b-instruct-q4_k_m.gguf",
            is_recommended: false,
            prompt_family: LocalLlmPromptFamily::Smollm2,
        },
        LocalLlmModelManifest {
            id: "granite-3.3-2b-instruct",
            name: "Granite 3.3 2B Instruct",
            description: "Compact Granite model with solid cleanup discipline and predictable formatting.",
            repo_id: "ibm-granite/granite-3.3-2b-instruct-GGUF",
            size_mb: 1420,
            quantization: "Q4_K_M",
            privacy_label: "Runs on this device",
            artifacts: GRANITE_2B_ARTIFACTS,
            primary_artifact: "granite-3.3-2b-instruct-Q4_K_M.gguf",
            is_recommended: false,
            prompt_family: LocalLlmPromptFamily::Granite33,
        },
        LocalLlmModelManifest {
            id: "granite-3.3-8b-instruct",
            name: "Granite 3.3 8B Instruct",
            description: "Biggest curated local cleanup model. Useful when quality matters more than load time.",
            repo_id: "ibm-granite/granite-3.3-8b-instruct-GGUF",
            size_mb: 4910,
            quantization: "Q4_K_M",
            privacy_label: "Runs on this device",
            artifacts: GRANITE_8B_ARTIFACTS,
            primary_artifact: "granite-3.3-8b-instruct-Q4_K_M.gguf",
            is_recommended: false,
            prompt_family: LocalLlmPromptFamily::Granite33,
        },
    ]
}

pub fn manifest_by_id(model_id: &str) -> Option<LocalLlmModelManifest> {
    built_in_model_manifests()
        .into_iter()
        .find(|manifest| manifest.id == model_id)
}

pub fn prompt_family_for_model(model_id: &str) -> Option<LocalLlmPromptFamily> {
    manifest_by_id(model_id).map(|manifest| manifest.prompt_family)
}

#[cfg(test)]
mod tests {
    use super::{built_in_model_manifests, manifest_by_id, prompt_family_for_model};
    use std::collections::HashSet;

    #[test]
    fn local_cleanup_catalog_ids_are_unique() {
        let manifests = built_in_model_manifests();
        let mut ids = HashSet::new();
        for manifest in manifests {
            assert!(ids.insert(manifest.id), "duplicate id {}", manifest.id);
            assert!(
                !manifest.artifacts.is_empty(),
                "manifest {} is missing artifacts",
                manifest.id
            );
            assert!(
                manifest
                    .artifacts
                    .iter()
                    .any(|artifact| artifact.filename == manifest.primary_artifact),
                "manifest {} primary artifact missing from artifact list",
                manifest.id
            );
        }
    }

    #[test]
    fn every_local_cleanup_model_resolves_a_prompt_family() {
        for manifest in built_in_model_manifests() {
            assert_eq!(
                prompt_family_for_model(manifest.id),
                Some(manifest.prompt_family),
                "missing family for {}",
                manifest.id
            );
            assert!(manifest_by_id(manifest.id).is_some());
        }
    }

    #[test]
    fn every_local_cleanup_artifact_has_a_well_formed_sha256() {
        // Downloaded GGUF files are executed by llama-server, so every
        // artifact must carry a checksum to verify against — and it must
        // actually look like a SHA256 hex digest, not a placeholder/typo,
        // since a malformed value would silently never match and brick the
        // model (every download would fail verification forever).
        for manifest in built_in_model_manifests() {
            for artifact in manifest.artifacts {
                let hash = artifact.sha256.unwrap_or_else(|| {
                    panic!(
                        "{} artifact {} is missing a sha256 checksum",
                        manifest.id, artifact.filename
                    )
                });
                assert_eq!(
                    hash.len(),
                    64,
                    "{} artifact {} sha256 is {} chars, expected 64",
                    manifest.id,
                    artifact.filename,
                    hash.len()
                );
                assert!(
                    hash.chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
                    "{} artifact {} sha256 is not lowercase hex: {hash}",
                    manifest.id,
                    artifact.filename
                );
            }
        }
    }
}
