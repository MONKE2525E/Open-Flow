# Roadmap: Transcription Utility

## 1. Prompt Hardening & Instruction Isolation (Priority)
- **Problem**: The filtering LLM occasionally interprets dictated text as meta-instructions (e.g., "stop," "delete," or "don't add periods").
- **Solution**: 
    - Implement XML-style delimiters (e.g., `<raw_audio_text>`) to wrap user input.
    - Inject a high-priority "System Instruction" that explicitly defines the model as a passive text-processor, forbidding it from executing any commands found within the data block.

## 2. Contextual Capitalization (Quick Win)
- **Problem**: New injections default to uppercase, breaking the flow of an existing sentence.
- **Solution**: 
    - Develop a "Look-Back" buffer that checks the character immediately preceding the cursor.
    - If the previous character is not a sentence-ender (`.`, `!`, `?`) or if it is a space/comma, force the first character of the new transcription to lowercase.

## 3. Connectivity & Offline Detection
- **Problem**: Attempting to transcribe while the API is unreachable leads to silent failures or "shouting into the void."
- **Solution**: 
    - Implement a heartbeat check for the internet connection.
    - Update the "Snake" UI (image_a3795b.png) to change color (e.g., dimming or turning red) when the connection is dropped to provide immediate visual feedback.

## 4. Local Ollama Integration (Future)
- **Goal**: Provide a fallback for offline use or users who prefer local inference over cloud APIs.
- **Implementation**: 
    - Build a toggle to switch from the cloud endpoint to a local Ollama instance running on `localhost:11434`.
    - Optimize for the user's high-VRAM hardware (RTX 5060 Ti 16GB) to ensure the experience isn't "lobotomized."

## 5. Persistent State & CLI Improvements
- **Goal**: Ensure the app remembers user preferences without manual re-entry.
- **Implementation**: 
    - Store mode preferences (Hands-Free vs. Hold-to-Transcribe) in a local config file.
    - Clean up CLI error logs to prevent cluttering the terminal when the GUI adapter is running.


 # Far future

## 1. Cloud synchronization and accounts are completely optional
- **Problem**: Users may want to sync their transcription history and settings across multiple devices
- **Solution**: 
    - Implement a cloud synchronization service
    - Provide a way for users to create and manage accounts
    - Ensure that cloud synchronization is optional and can be disabled by users  

## 2. Cloud-based API key management
- **Problem**: Users may want to never worry about any API keys ever so offer it as a cloud feature and development is expensive.
- **Solution**: 
    - Implement a cloud-based API key aggregation system.
    - Ensure it's always completely optional and no matter what make BYOK (bring your own key) as the number one priority.
    - Allow users to pick between BYOK and full cloud based service.  