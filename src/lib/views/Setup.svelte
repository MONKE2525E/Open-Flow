<script lang="ts">
  import { invoke } from '../tauri';
  import { onMount } from 'svelte';
  import { appStore } from '../stores';
  import { getSetupCalibrationCopy } from '../calibrationCopy';
  import { saveSetting, type CleanupIntensity, type ProviderId, type ProviderModelMap, type ToneId } from '../settings';
  import { getTranscriptionLanguageLabel, transcriptionLanguages, type TranscriptionLanguageCode } from '../transcriptionLanguages';
  import { isMac } from '../platform';
  import { motionMs, pageSwap } from '../motion';
  import { loadHotkey } from '../hotkey.svelte';
  import { isCalibrating, calibratedGain } from '../calibration';
  import { providers, cleanupCards, toneCards, SETUP_APPEARANCE_MODE } from '../setup/setupData';
  import type { Preset } from '../components/settings/modelPresets';
  import { splitModelId } from '../components/settings/models';
  import SetupShell from '../setup/SetupShell.svelte';
  import IntroStep from '../setup/steps/IntroStep.svelte';
  import ProviderStep from '../setup/steps/ProviderStep.svelte';
  import ApiKeyStep from '../setup/steps/ApiKeyStep.svelte';
  import PermissionsStep from '../setup/steps/PermissionsStep.svelte';
  import ModelsStep from '../setup/steps/ModelsStep.svelte';
  import WritingStyleStep from '../setup/steps/WritingStyleStep.svelte';
  import LanguageStep from '../setup/steps/LanguageStep.svelte';
  import AudioEnvironmentStep from '../setup/steps/AudioEnvironmentStep.svelte';
  import CalibrationStep from '../setup/steps/CalibrationStep.svelte';
  import TryItStep from '../setup/steps/TryItStep.svelte';
  import DoneStep from '../setup/steps/DoneStep.svelte';

  const TOTAL_STEPS = isMac ? 9 : 8;
  const providerStep = 1;
  const apiKeyStep = 2;
  const permissionStep = isMac ? 3 : -1;
  const modelsStep = isMac ? 4 : 3;
  const writingStyleStep = isMac ? 5 : 4;
  const languageStep = isMac ? 6 : 5;
  const audioEnvStep = isMac ? 7 : 6;
  const calibrationStep = isMac ? 8 : 7;
  const tryItStep = isMac ? 9 : 8;
  const doneStep = TOTAL_STEPS + 1;

  let step = $state(0);
  let direction = $state<'forward' | 'back'>('forward');
  let animating = $state(false);

  let provider = $state<ProviderId>('groq');
  let apiKeyDraft = $state('');
  let apiKeyMode = $state<'fork' | 'tutorial' | 'paste'>('fork');
  let keySaved = $state(false);
  let providerKeyStatus = $state<Record<ProviderId, boolean>>({
    groq: false,
    openai: false,
    google: false,
    assemblyai: false,
    local: true,
  });
  let previousProvider = $state<ProviderId | null>(null);
  let keySaving = $state(false);
  let keyError = $state('');
  let showKey = $state(false);
  let keyValidation = $state<{ status: 'idle' | 'checking' | 'valid' | 'invalid' | 'unknown'; message: string }>({ status: 'idle', message: '' });

  let allCoreGranted = $state(false);
  let modelPreset = $state<Preset | null>(null);

  let cleanupIntensity = $state<CleanupIntensity>('medium');
  let tone = $state<ToneId>('casual');
  let language = $state<TranscriptionLanguageCode>('en');
  let usesHeadphones = $state(true);
  let saveError = $state('');

  let providerDisplayName = $derived(providers.find((p) => p.id === provider)?.name ?? '');
  const setupCalibrationCopy = getSetupCalibrationCopy();
  let cleanupName = $derived(cleanupCards.find((c) => c.id === cleanupIntensity)?.name ?? '');
  let toneName = $derived(toneCards.find((t) => t.id === tone)?.name ?? '');
  let languageLabel = $derived(getTranscriptionLanguageLabel(language));

  onMount(async () => {
    void loadHotkey();
    try {
      const [
        savedLanguage, savedProvider, savedIntensity, savedTone, keyStatus, savedMute,
      ] = await Promise.all([
        invoke<TranscriptionLanguageCode | null>('get_setting', { key: 'transcription_language' }),
        invoke<ProviderId | null>('get_setting', { key: 'transcription_provider' }),
        invoke<CleanupIntensity | null>('get_setting', { key: 'cleanup_intensity' }),
        invoke<ToneId | null>('get_setting', { key: 'default_tone' }),
        invoke<Record<ProviderId, boolean> | null>('get_api_key_status'),
        invoke<boolean | null>('get_setting', { key: 'mute_audio' }),
      ]);
      if (savedLanguage && transcriptionLanguages.some((o) => o.code === savedLanguage)) language = savedLanguage;
      if (savedProvider && providers.some((p) => p.id === savedProvider)) provider = savedProvider;
      if (savedIntensity && cleanupCards.some((c) => c.id === savedIntensity)) cleanupIntensity = savedIntensity;
      if (savedTone && toneCards.some((t) => t.id === savedTone)) tone = savedTone;
      if (keyStatus) {
        providerKeyStatus = { ...providerKeyStatus, ...keyStatus, local: true };
      }
      // Muting implies speakers — that's the only reason the setting is on.
      if (savedMute === true) usesHeadphones = false;
    } catch {}
  });

  $effect(() => {
    if (previousProvider !== null && previousProvider !== provider) {
      apiKeyDraft = '';
      keyError = '';
      keyValidation = { status: 'idle', message: '' };
      apiKeyMode = 'fork';
    }
    previousProvider = provider;
    keySaved = provider === 'local' ? true : !!providerKeyStatus[provider];
    if (provider === 'local') {
      keyError = '';
      keyValidation = { status: 'idle', message: '' };
    }
  });

  function delay(ms: number) { return new Promise((r) => setTimeout(r, ms)); }

  // Slide direction follows travel: going forward, the old step leaves to the
  // left and the new one arrives from the right. The previous version sent both
  // the same way, which is why it read as a flat fade rather than movement.
  const STEP_SHIFT = 26;
  const stepInParams = $derived({
    axis: 'x' as const,
    distance: direction === 'forward' ? STEP_SHIFT : -STEP_SHIFT,
    duration: 300,
  });
  const stepOutParams = $derived({
    axis: 'x' as const,
    distance: direction === 'forward' ? -STEP_SHIFT : STEP_SHIFT,
    duration: 190,
  });

  async function animateTo(target: number, dir: 'forward' | 'back') {
    if (animating) return;
    direction = dir;
    animating = true;
    step = target;
    await delay(motionMs(300));
    animating = false;
  }

  const goNext = () => animateTo(step + 1, 'forward');
  const skip = goNext;

  function goBack() {
    // Always leaves the step. The API key step's fork/tutorial/paste states have
    // their own in-page controls; making Back unwind those instead trapped
    // people on step 2.
    if (step > 0) void animateTo(step - 1, 'back');
  }

  function jumpToStep(target: number) {
    if (target === step) return;
    void animateTo(target, target < step ? 'back' : 'forward');
  }

  async function saveKey() {
    if (provider === 'local') return;
    const trimmed = apiKeyDraft.trim();
    if (!trimmed) return;
    keySaving = true;
    keyError = '';
    keyValidation = { status: 'idle', message: '' };
    try {
      await invoke('save_api_key', { provider, key: trimmed });
      providerKeyStatus = { ...providerKeyStatus, [provider]: true };
      keySaved = true;
      apiKeyDraft = '';
    } catch {
      keyError = 'Could not save the key. Check your connection and try again.';
      keySaving = false;
      return;
    }
    keySaving = false;
    void validateKey(trimmed);
  }

  async function validateKey(key: string) {
    if (provider === 'local') {
      keyValidation = { status: 'idle', message: '' };
      return;
    }
    keyValidation = { status: 'checking', message: '' };
    try {
      const result = await invoke<{ ok: boolean; status: 'valid' | 'invalid' | 'unknown'; message: string }>('validate_api_key', { provider, key });
      keyValidation = { status: result.status, message: result.message };
    } catch {
      keyValidation = { status: 'unknown', message: "Couldn't verify the key right now." };
    }
  }

  /** Base model lists the wizard seeds, plus whatever the chosen preset points at. */
  function modelsByProvider(...selected: string[]): ProviderModelMap {
    const map: ProviderModelMap = {
      groq: ['whisper-large-v3-turbo', 'whisper-large-v3'],
      openai: ['gpt-4o-mini-transcribe', 'gpt-4o-transcribe'],
      google: ['gemini-2.5-flash', 'gemini-3.5-flash'],
      assemblyai: [],
      local: ['parakeet-v3'],
    };
    for (const id of selected) {
      const parsed = splitModelId(id);
      if (!parsed) continue;
      const providerModels = map[parsed.provider];
      if (!providerModels) continue;
      if (!providerModels.includes(parsed.model)) providerModels.push(parsed.model);
    }
    return map;
  }

  function cleanupModelsByProvider(...selected: string[]): ProviderModelMap {
    const map: ProviderModelMap = {
      groq: ['llama-3.3-70b-versatile', 'llama-3.1-8b-instant'],
      openai: ['gpt-4o-mini', 'gpt-4o'],
      google: ['gemini-2.5-flash', 'gemini-3.5-flash'],
      assemblyai: [],
      local: ['qwen2.5-3b-instruct'],
    };
    for (const id of selected) {
      const parsed = splitModelId(id);
      if (!parsed) continue;
      const providerModels = map[parsed.provider];
      if (!providerModels) continue;
      if (!providerModels.includes(parsed.model)) providerModels.push(parsed.model);
    }
    return map;
  }

  async function finish() {
    const target = modelPreset?.target ?? null;
    const providerDefaultTranscription = provider === 'local'
      ? 'local/parakeet-v3'
      : provider === 'openai'
        ? 'openai/gpt-4o-transcribe'
        : provider === 'google'
          ? 'google/gemini-3.5-flash'
          : 'groq/whisper-large-v3-turbo';
    const providerDefaultCleanup = provider === 'local'
      ? 'local/qwen2.5-3b-instruct'
      : provider === 'openai'
        ? 'openai/gpt-4o-mini'
        : provider === 'google'
          ? 'google/gemini-3.5-flash'
          : 'groq/llama-3.3-70b-versatile';

    // The Models step is the more specific answer, so its target wins over the
    // provider-derived defaults whenever one was chosen.
    const transcriptionDefaultModel = target?.transcriptionDefaultModel ?? providerDefaultTranscription;
    const cleanupDefaultModel = target?.cleanupDefaultModel ?? providerDefaultCleanup;
    const transcriptionProvider = splitModelId(transcriptionDefaultModel)?.provider ?? provider;
    const cleanupProvider = splitModelId(cleanupDefaultModel)?.provider ?? provider;

    // Intensity 'none' and cleanup_enabled=false are ANDed by the pipeline
    // (see should_run_cleanup_llm), so keep the Settings toggle agreeing with
    // what the wizard was actually told. A preset with no cleanup model (e.g.
    // "Transcription only") also forces it off.
    const cleanupEnabled = cleanupIntensity !== 'none' && (target ? target.cleanupEnabled : true);
    // Speakers means playback bleeds into the mic; headphones means it can't.
    const silenceOtherAudio = !usesHeadphones;

    saveError = '';
    try {
      const settingsToSave: Array<() => Promise<unknown>> = [
        () => saveSetting('cleanup_intensity', cleanupIntensity),
        () => saveSetting('default_tone', tone),
        () => saveSetting('cleanup_enabled', cleanupEnabled),
        () => saveSetting('transcription_provider', transcriptionProvider),
        () => saveSetting('transcription_model', transcriptionDefaultModel),
        () => saveSetting('transcription_default_model', transcriptionDefaultModel),
        () => saveSetting('transcription_models_by_provider', modelsByProvider(transcriptionDefaultModel, ...(target?.transcriptionFallbacks ?? []))),
        () => saveSetting('transcription_fallback_models', target?.transcriptionFallbacks ?? []),
        () => saveSetting('dual_transcription_enabled', target?.dualTranscription ?? false),
        () => saveSetting('transcription_language', language),
        () => saveSetting('cleanup_provider', cleanupProvider),
        () => saveSetting('cleanup_model', cleanupDefaultModel),
        () => saveSetting('cleanup_default_model', cleanupDefaultModel),
        () => saveSetting('cleanup_models_by_provider', cleanupModelsByProvider(cleanupDefaultModel, ...(target?.cleanupFallbacks ?? []))),
        () => saveSetting('cleanup_fallback_models', cleanupEnabled ? (target?.cleanupFallbacks ?? []) : []),
        () => saveSetting('appearance_mode', SETUP_APPEARANCE_MODE),
        () => saveSetting('mute_audio', silenceOtherAudio),
        () => saveSetting('pause_media_during_dictation', silenceOtherAudio),
        // The wizard no longer asks about these one by one; they are the
        // recommended defaults and are disclosed on the Done screen.
        () => saveSetting('noise_reduction', true),
        () => saveSetting('contextual_caps_enabled', true),
        () => saveSetting('auto_spacing_enabled', true),
        () => saveSetting('caps_lock_uppercase_enabled', true),
        () => saveSetting('app_context_hint', true),
        () => saveSetting('auto_learn_enabled', true),
      ];
      for (const save of settingsToSave) await save();
      await invoke('set_autostart', { enabled: true });
    } catch (err) {
      // Previously this was swallowed, leaving a half-written config behind an
      // apparently successful setup. Stop before marking setup complete.
      console.error('Failed to save setup settings:', err);
      saveError = 'Some settings could not be saved. Check that Verenu can write to its data folder, then try again.';
      return;
    }

    try {
      await saveSetting('setup_complete', true);
    } catch (err) {
      console.error('Failed to mark setup complete:', err);
      saveError = 'Your choices were saved, but setup could not be marked complete. Try again.';
      return;
    }

    appStore.appearanceMode = SETUP_APPEARANCE_MODE;
    appStore.cleanupEnabled = cleanupEnabled;
    appStore.setupComplete = true;
  }

  type HeaderInfo = { title: string; subtitle: string; name: string } | null;
  function headerFor(s: number): HeaderInfo {
    if (s === providerStep) return { name: 'Provider', title: 'Choose your AI provider', subtitle: 'This powers both transcription and text cleanup. You can switch anytime in Settings.' };
    if (s === apiKeyStep) {
      if (provider === 'local') {
        return { name: 'Local', title: 'No API key needed for local transcription', subtitle: 'Download Parakeet V3 later in Settings → Models. Cleanup Off keeps the transcript local too.' };
      }
      if (apiKeyMode === 'tutorial') {
        return { name: 'API Key', title: `Creating a ${providerDisplayName} API key`, subtitle: 'Follow along in your browser, then come back and paste the key.' };
      }
      if (apiKeyMode === 'paste') {
        return { name: 'API Key', title: `Paste your ${providerDisplayName} API key`, subtitle: 'Stored in your OS credential manager — it never leaves this machine.' };
      }
      return { name: 'API Key', title: `Connect ${providerDisplayName}`, subtitle: 'Verenu needs a key to send audio for transcription.' };
    }
    if (isMac && s === permissionStep) return { name: 'Permissions', title: 'Check your macOS permissions', subtitle: 'Verenu needs these to hear your voice and type for you.' };
    if (s === modelsStep) return { name: 'Models', title: 'How should Verenu run?', subtitle: 'Each option picks a transcription and cleanup model for you.' };
    if (s === writingStyleStep) return { name: 'Writing Style', title: 'How should your dictation sound?', subtitle: 'Cleanup intensity and tone shape every transcription. You can override both per-app later.' };
    if (s === languageStep) return { name: 'Language', title: 'What language will you dictate in?', subtitle: "This is the language Verenu expects to hear. The app's own interface stays in English." };
    if (s === audioEnvStep) return { name: 'Audio', title: 'Headphones or speakers?', subtitle: 'This decides whether Verenu needs to silence your other audio while you dictate.' };
    if (s === calibrationStep) return { name: 'Microphone', title: setupCalibrationCopy.title, subtitle: setupCalibrationCopy.subtitle };
    if (s === tryItStep) return { name: 'Try It', title: 'Give it a try', subtitle: 'Test the full pipeline, end to end, before you go.' };
    return null;
  }

  type ActionBarConfig = {
    leftLabel: string | null;
    leftDisabled: boolean;
    onLeft: () => void;
    rightLabel: string;
    rightDisabled: boolean;
    rightLg: boolean;
    rightGlow: boolean;
    onRight: () => void;
  };

  function bar(partial: Partial<ActionBarConfig> & { rightLabel: string; onRight: () => void }): ActionBarConfig {
    return {
      leftLabel: null,
      leftDisabled: false,
      onLeft: skip,
      rightDisabled: false,
      rightLg: false,
      rightGlow: false,
      ...partial,
    };
  }

  let actionBar = $derived.by((): ActionBarConfig => {
    if (step === 0) return bar({ rightLabel: 'Get Started', rightLg: true, onRight: goNext });
    if (step === doneStep) return bar({ rightLabel: 'Start dictating', rightLg: true, onRight: finish });
    if (step === providerStep) return bar({ rightLabel: 'Next', onRight: goNext });
    if (step === apiKeyStep) {
      if (provider === 'local') return bar({ rightLabel: 'Continue', onRight: goNext });
      if (keySaving) return bar({ leftLabel: 'Skip for now', rightLabel: 'Saving…', rightDisabled: true, onRight: () => {} });
      if (apiKeyDraft.trim()) return bar({ leftLabel: 'Skip for now', rightLabel: 'Save Key', onRight: saveKey });
      // On the fork the two cards are the real choice, so the bar offers the way
      // out rather than a second, disabled "Continue" that strands anyone who
      // does not pick a card.
      if (apiKeyMode === 'fork' && !keySaved) return bar({ rightLabel: "I'll add it later", onRight: goNext });
      if (apiKeyMode === 'paste') return bar({ leftLabel: 'Skip for now', rightLabel: 'Continue', onRight: goNext });
      return bar({ leftLabel: 'Skip for now', rightLabel: 'Continue', onRight: goNext });
    }
    if (isMac && step === permissionStep) {
      return bar({
        rightLabel: allCoreGranted ? 'Next' : 'Grant permissions to continue',
        rightDisabled: !allCoreGranted,
        rightGlow: allCoreGranted,
        onRight: goNext,
      });
    }
    if (step === calibrationStep) {
      const calibrated = $calibratedGain !== null;
      return bar({
        leftLabel: setupCalibrationCopy.skipButton,
        leftDisabled: $isCalibrating,
        rightLabel: calibrated ? setupCalibrationCopy.continueButton : 'Next',
        rightDisabled: $isCalibrating,
        onRight: goNext,
      });
    }
    if (step === tryItStep) return bar({ leftLabel: 'Skip for now', rightLabel: 'Next', onRight: goNext });
    // Writing style, language and audio all have a working default already —
    // "Skip for now" next to "Next" would be two words for the same action.
    return bar({ rightLabel: 'Next', onRight: goNext });
  });

  const canGoBack = $derived(step > 0 && step <= TOTAL_STEPS);
</script>

<SetupShell
  {step}
  totalSteps={TOTAL_STEPS}
  header={headerFor(step)}
  onDotClick={jumpToStep}
>
  {#snippet left()}
    {#if canGoBack}
      <button class="btn-back" onclick={goBack} disabled={animating || $isCalibrating}>
        <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="15 18 9 12 15 6"/></svg>
        Back
      </button>
    {/if}
    {#if actionBar.leftLabel}
      <button class="btn-skip" onclick={actionBar.onLeft} disabled={actionBar.leftDisabled || animating}>{actionBar.leftLabel}</button>
    {/if}
  {/snippet}

  {#snippet right()}
    {#if saveError}
      <span class="setup-save-error" role="alert">{saveError}</span>
    {/if}
    <button
      class="btn-primary"
      class:btn-lg={actionBar.rightLg}
      class:btn-primary--glow={actionBar.rightGlow}
      onclick={actionBar.onRight}
      disabled={actionBar.rightDisabled || animating}
    >{actionBar.rightLabel}</button>
  {/snippet}

  {#key step}
  <div class="step-wrap" in:pageSwap={stepInParams} out:pageSwap={stepOutParams}>
    {#if step === 0}
      <IntroStep />
    {:else if step === providerStep}
      <ProviderStep bind:provider />
    {:else if step === apiKeyStep}
      <ApiKeyStep
        {provider}
        providerName={providerDisplayName}
        bind:apiKeyDraft
        bind:showKey
        bind:mode={apiKeyMode}
        {keySaved}
        {keySaving}
        {keyError}
        {keyValidation}
      />
    {:else if isMac && step === permissionStep}
      <PermissionsStep {provider} bind:allCoreGranted />
    {:else if step === modelsStep}
      <ModelsStep apiKeyStatus={providerKeyStatus} bind:preset={modelPreset} onOpenApiKeys={() => jumpToStep(apiKeyStep)} />
    {:else if step === writingStyleStep}
      <WritingStyleStep bind:intensity={cleanupIntensity} bind:tone />
    {:else if step === languageStep}
      <LanguageStep bind:language />
    {:else if step === audioEnvStep}
      <AudioEnvironmentStep bind:usesHeadphones />
    {:else if step === calibrationStep}
      <CalibrationStep />
    {:else if step === tryItStep}
      <TryItStep />
    {:else if step === doneStep}
      <DoneStep
        providerName={providerDisplayName}
        {cleanupName}
        {toneName}
        {languageLabel}
        {usesHeadphones}
        micGain={$calibratedGain}
        hasKey={keySaved}
        presetName={modelPreset?.name ?? ''}
      />
    {/if}
  </div>
  {/key}
</SetupShell>

<style>
  .step-wrap {
    width: 100%;
    display: flex;
    justify-content: center;
  }

  .setup-save-error {
    font-size: 12px;
    color: var(--danger);
    max-width: 320px;
    line-height: 1.4;
    text-align: right;
  }

</style>
