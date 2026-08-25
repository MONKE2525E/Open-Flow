# Local Transcription

Verenu now supports local transcription as a first-class transcription backend.

## Beta Scope

This beta ships one clear local path:

- Local transcription with `local/parakeet-v3`
- Optional cloud cleanup after local transcription
- `Cleanup: Off` for a no-cleanup path

What this beta does **not** claim:

- It is not "fully local" unless cleanup is also Off
- It does not include local cleanup LLMs
- It does not expose Moonshine, Whisper.cpp, or custom local models as the default path

Those advanced models are present behind the advanced local section for follow-up work and compatibility testing.

## How It Works

1. Verenu records audio locally.
2. If transcription is set to `Local/offline`, Verenu sends 16 kHz mono PCM directly to the local STT adapter instead of re-decoding a WAV.
3. The adapter loads the selected local model from `models/stt/` under the app-data directory.
4. The local model returns raw transcription text.
5. Verenu either:
   - keeps that text as-is when `Cleanup: Off`, or
   - sends only the transcript text to the selected cloud cleanup provider.
6. Verenu pastes the final result and stores history locally.

## Privacy Modes

### Local transcription + Cleanup Off

After the model download, both audio and transcript stay on the device.

### Local transcription + cloud cleanup

Audio stays on the device.

The transcript text, cleanup instructions, and related cleanup context leave the device because the cleanup provider needs them.

### Cloud transcription

Recorded audio leaves the device and goes to the selected transcription provider.

## Models

### Recommended

- `local/parakeet-v3`

### Advanced

- `local/moonshine-base`
- `local/whisper-custom`

Advanced models are intentionally de-emphasized in this beta. Parakeet V3 is the supported path that the rest of the UX is built around.

## Downloads and Storage

- Local models are stored in `models/stt/` inside Verenu's app-data directory
- Downloads use resumable HTTP range requests when the server supports them
- Partial archives use `.partial`
- In-progress extraction uses `.extracting`
- Models are only treated as usable after download verification and atomic install finish

## Failure Behavior

- Missing selected local model: Verenu shows `Download the selected local model.`
- Retryable local runtime failures can fall back to a configured cloud transcription model
- Non-retryable local model and configuration failures do not silently fall through to cloud

## Current Limits

- No Linux-specific local support work in this beta
- No local cleanup model support
- Local transcription quality, speed, and RAM use still depend heavily on the machine and the selected model
