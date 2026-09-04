# Local transcription and cleanup

Verenu supports on-device transcription and on-device cleanup. Models and the local cleanup runtime are downloaded from Settings -> Models when you choose them.

## How the local path works

1. Verenu records the audio on your device.
2. A selected `local/<model>` transcription model receives the captured audio through the local STT adapter.
3. The local model returns raw transcript text.
4. With Cleanup Off, Verenu keeps that text as-is. With cleanup enabled, it sends the text to either a downloaded local cleanup model or a cloud cleanup provider.
5. Verenu pastes the final text and stores the dictation in local history.

Local transcription with local cleanup keeps the dictation data on the device after the model files have been downloaded. Local transcription with cloud cleanup keeps the audio local but sends the transcript and cleanup context to the selected cloud provider.

## Local transcription models

The built-in local transcription catalog currently includes:

- Parakeet V3 and Parakeet V2
- Moonshine Tiny, Base, Small, and Medium
- SenseVoice
- GigaAM V3
- Canary 180M Flash and Canary 1B V2
- Cohere

The model picker shows download state, verifies completed downloads before marking them ready, and allows a downloaded model to be cancelled or removed.

## Local cleanup models

The built-in local cleanup catalog currently includes:

- Gemma 4 E2B and E4B
- Qwen 2.5 0.5B, 1.5B, 3B, and 7B Instruct
- Phi-3 Mini 4K Instruct
- SmolLM2 360M and 1.7B Instruct
- Granite 3.3 2B and 8B Instruct

All local cleanup models share one downloaded runtime. Verenu downloads that runtime once, then manages the individual cleanup model files separately.

## Storage and downloads

- Local transcription models are stored in the app-data `models/stt` directory.
- Local cleanup models are stored in the app-data `models/cleanup` directory.
- The local cleanup runtime is stored in the app-data `models/bin` directory.
- Downloads are verified and installed atomically before a model becomes selectable.
- Settings -> Models provides download, cancel, and delete actions for local models and the cleanup runtime.

## Fallback behavior

- A missing selected model produces a download prompt in the model picker.
- A retryable local transcription failure can move to a configured cloud transcription fallback.
- A non-retryable local model or configuration error is reported instead of silently switching to the cloud path.
- Cloud fallback models still require the corresponding provider key.

## Platform limits

Local model downloads and inference are currently available on Windows and Apple Silicon Macs. Local models are gated off on Intel Macs until that path has been validated on real hardware. Linux is not a supported desktop target.

Speed, memory use, and output quality depend on the selected model and the computer running it. Larger local cleanup models need more memory and may take longer to answer.

## Choosing a private path

For a fully on-device dictation path, select a local transcription model, select a local cleanup model or turn Cleanup Off, and download the required files. Model downloads and any normal update or service-status requests are separate from dictation processing.

See [Privacy & Data](PRIVACY_SUMMARY.md) for the short privacy summary and [Data And Privacy](DATA_AND_PRIVACY.md) for the complete data map.
