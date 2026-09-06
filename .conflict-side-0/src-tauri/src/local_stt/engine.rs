use super::model::{LocalSttEngineType, LocalSttModelManifest};
use std::path::Path;
use transcribe_rs::onnx::canary::CanaryModel;
use transcribe_rs::onnx::cohere::CohereModel;
use transcribe_rs::onnx::gigaam::GigaAMModel;
use transcribe_rs::onnx::moonshine::{MoonshineModel, MoonshineVariant, StreamingModel};
use transcribe_rs::onnx::parakeet::ParakeetModel;
use transcribe_rs::onnx::sense_voice::SenseVoiceModel;
use transcribe_rs::onnx::Quantization;
use transcribe_rs::{SpeechModel, TranscribeOptions};

/// Number of inference threads for streaming Moonshine models. Matches the
/// crate's own example default (examples/moonshine_streaming.rs) — there is
/// no per-platform tuning need here, ONNX Runtime CPU inference scales fine
/// at this thread count for a model this small.
const MOONSHINE_STREAMING_THREADS: usize = 4;

pub enum LoadedLocalSttEngine {
    Parakeet(ParakeetModel),
    Moonshine(MoonshineModel),
    MoonshineStreaming(StreamingModel),
    SenseVoice(SenseVoiceModel),
    GigaAm(GigaAMModel),
    Canary(CanaryModel),
    Cohere(CohereModel),
}

impl LoadedLocalSttEngine {
    pub fn transcribe(
        &mut self,
        samples: &[f32],
        sample_rate: u32,
        language: &str,
    ) -> anyhow::Result<String> {
        if sample_rate != 16_000 {
            anyhow::bail!("local transcription requires 16 kHz mono PCM")
        }
        let options = TranscribeOptions {
            language: if language.trim().is_empty() {
                None
            } else {
                Some(language.to_string())
            },
            ..Default::default()
        };
        let result = match self {
            Self::Parakeet(model) => model.transcribe(samples, &options),
            Self::Moonshine(model) => model.transcribe(samples, &options),
            Self::MoonshineStreaming(model) => model.transcribe(samples, &options),
            Self::SenseVoice(model) => model.transcribe(samples, &options),
            Self::GigaAm(model) => model.transcribe(samples, &options),
            Self::Canary(model) => model.transcribe(samples, &options),
            Self::Cohere(model) => model.transcribe(samples, &options),
        }?;
        Ok(result.text.trim().to_string())
    }
}

pub fn load_engine(
    manifest: &LocalSttModelManifest,
    model_path: &Path,
) -> anyhow::Result<LoadedLocalSttEngine> {
    match manifest.engine_type {
        LocalSttEngineType::Parakeet => Ok(LoadedLocalSttEngine::Parakeet(ParakeetModel::load(
            model_path,
            &Quantization::Int8,
        )?)),
        LocalSttEngineType::Moonshine => Ok(LoadedLocalSttEngine::Moonshine(MoonshineModel::load(
            model_path,
            MoonshineVariant::Base,
            &Quantization::default(),
        )?)),
        LocalSttEngineType::MoonshineStreaming => Ok(LoadedLocalSttEngine::MoonshineStreaming(
            StreamingModel::load(model_path, MOONSHINE_STREAMING_THREADS, &Quantization::Int8)?,
        )),
        LocalSttEngineType::SenseVoice => Ok(LoadedLocalSttEngine::SenseVoice(
            SenseVoiceModel::load(model_path, &Quantization::Int8)?,
        )),
        LocalSttEngineType::GigaAm => Ok(LoadedLocalSttEngine::GigaAm(GigaAMModel::load(
            model_path,
            &Quantization::Int8,
        )?)),
        LocalSttEngineType::Canary => Ok(LoadedLocalSttEngine::Canary(CanaryModel::load(
            model_path,
            &Quantization::Int8,
        )?)),
        LocalSttEngineType::Cohere => Ok(LoadedLocalSttEngine::Cohere(CohereModel::load(
            model_path,
            &Quantization::Int8,
        )?)),
    }
}
