# Your First Dictation

Verenu works with a single hold-to-record hotkey — no clicking, no app switching.

## The hotkey

- **Windows**: hold <kbd>Ctrl</kbd> + <kbd>Windows</kbd>
- **macOS**: hold <kbd>Fn</kbd> + <kbd>Control</kbd>

Click into any text field — a document, an email, a chat box — then hold the hotkey and start talking.

## What happens while you hold it

A small floating pill appears on screen with animated bars that move with your voice in real time. This is just a visual indicator that Verenu is listening; it doesn't take focus away from the app you're typing into.

## What happens when you release it

1. The pill switches to a processing state while Verenu transcribes your audio and runs it through cleanup.
2. The cleaned-up text is pasted directly into whatever field had focus when you started recording — even if a few seconds have passed.
3. The dictation is saved to your local history automatically, so you can find it again later.

## If nothing happens

Verenu silently skips processing — with no error and no output — in two cases:

- **The recording was too short** (under ~0.7 seconds). This avoids the transcription model hallucinating text from a near-empty clip.
- **The recording was too quiet** (near-silence). This usually means the hotkey was triggered accidentally, or your microphone input level is very low.

If you consistently get no output, check your microphone input level in **Settings → Microphone**.

## macOS tip

The <kbd>Fn</kbd> key can also open the emoji picker on macOS. To avoid that popping up every time you dictate, go to **System Settings → Keyboard** and set "Press 🌐 key to: **Do Nothing**".

If you'd rather keep the emoji picker on <kbd>Fn</kbd>, you can change Verenu's hotkey instead. In Verenu, go to **Settings → General** — the **Hotkey** option is right at the top. Click it, then press the new key combination you want to use.

## Next step

Want more control over how your text is cleaned up? See [Cleanup Levels](CLEANUP_LEVELS.md).

## Related Docs

<p align="center">
  <a href="API_KEYS.md"><img alt="API Keys" src="https://img.shields.io/badge/Back-API%20Keys-7e7266"></a>
  <a href="CLEANUP_LEVELS.md"><img alt="Cleanup Levels" src="https://img.shields.io/badge/Next-Cleanup%20Levels-c44632"></a>
  <a href="TROUBLESHOOTING.md"><img alt="Troubleshooting" src="https://img.shields.io/badge/Help-Troubleshooting-5b554a"></a>
  <a href="README.md"><img alt="Docs Index" src="https://img.shields.io/badge/Docs-Index-2b2422"></a>
</p>
