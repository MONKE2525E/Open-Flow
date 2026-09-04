# Your first dictation

Verenu works with a single hold-to-record hotkey. You do not need to click the app or switch windows.

## The hotkey

- **Windows**: hold <kbd>Ctrl</kbd> + <kbd>Windows</kbd>
- **macOS**: hold <kbd>Option</kbd> + <kbd>Space</kbd>

Click into any text field, then hold the hotkey and start talking.

## While you hold it

A small floating pill appears with animated bars that respond to your voice. It shows that Verenu is listening without taking focus from the app you are typing in.

## When you release it

1. Verenu checks the recording, transcribes it with the selected transcription model, and runs the optional cleanup step.
2. It pastes the final text into the field that had focus when you started recording.
3. It saves the dictation to local history.

## If no text appears

Verenu rejects recordings that are too short or too quiet, and the pill reports which check failed:

- **Too short**: the recording is under about 0.7 seconds.
- **Too quiet**: the recording is near silence, often because the hotkey was triggered accidentally or the microphone level is low.

If this happens repeatedly, open **Settings -> Audio**, check the selected microphone, and run microphone calibration.

## macOS permissions

If text is not inserted on macOS, open **Settings -> Permissions** and confirm that Verenu has Microphone and Accessibility access. Accessibility is needed for focused-text reads and text injection.

## Next step

For more control over how text is edited, see [Cleanup Levels](CLEANUP_LEVELS.md).

## Related docs

<p align="center">
  <a href="API_KEYS.md"><img alt="API Keys" src="https://img.shields.io/badge/Back-API%20Keys-7e7266"></a>
  <a href="CLEANUP_LEVELS.md"><img alt="Cleanup Levels" src="https://img.shields.io/badge/Next-Cleanup%20Levels-c44632"></a>
  <a href="TROUBLESHOOTING.md"><img alt="Troubleshooting" src="https://img.shields.io/badge/Help-Troubleshooting-5b554a"></a>
  <a href="README.md"><img alt="Docs Index" src="https://img.shields.io/badge/Docs-Index-2b2422"></a>
</p>
