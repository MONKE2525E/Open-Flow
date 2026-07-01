<script lang="ts">
  import { invoke } from '../tauri';
  import { onMount } from 'svelte';
  import { appStore } from '../stores';
  import { getSetupCalibrationCopy } from '../calibrationCopy';
  import { saveSetting, type AppearanceMode, type CleanupIntensity, type ProviderId, type ToneId } from '../settings';
  import { getTranscriptionLanguageLabel, transcriptionLanguages, type TranscriptionLanguageCode } from '../transcriptionLanguages';
  import { isMac } from '../platform';
  import { isCalibrating, calibratedGain } from '../calibration';
  import { providers, cleanupCards, toneCards, appearanceModes } from '../setup/setupData';
  import SetupShell from '../setup/SetupShell.svelte';
  import IntroStep from '../setup/steps/IntroStep.svelte';
  import ProviderStep from '../setup/steps/ProviderStep.svelte';
  import ApiKeyStep from '../setup/steps/ApiKeyStep.svelte';
  import PermissionsStep from '../setup/steps/PermissionsStep.svelte';
  import WritingStyleStep from '../setup/steps/WritingStyleStep.svelte';
  import AppearanceStep from '../setup/steps/AppearanceStep.svelte';
  import QuickSettingsStep from '../setup/steps/QuickSettingsStep.svelte';
  import CalibrationStep from '../setup/steps/CalibrationStep.svelte';
  import TryItStep from '../setup/steps/TryItStep.svelte';
  import DoneStep from '../setup/steps/DoneStep.svelte';

  const TOTAL_STEPS = isMac ? 8 : 7;
  const providerStep = 1;
  const apiKeyStep = 2;
  const permissionStep = isMac ? 3 : -1;
  const writingStyleStep = isMac ? 4 : 3;
  const appearanceStep = isMac ? 5 : 4;
  const quickSettingsStep = isMac ? 6 : 5;
  const calibrationStep = isMac ? 7 : 6;
  const tryItStep = isMac ? 8 : 7;
  const doneStep = TOTAL_STEPS + 1;

  let step = $state(0);
  let direction = $state<'forward' | 'back'>('forward');
  let animating = $state(false);
  let visible = $state(true);

  let provider = $state<ProviderId>('groq');
  let apiKeyDraft = $state('');
  let keySaved = $state(false);
  let keySaving = $state(false);
  let keyError = $state('');
  let showKey = $state(false);
  let keyValidation = $state<{ status: 'idle' | 'checking' | 'valid' | 'invalid' | 'unknown'; message: string }>({ status: 'idle', message: '' });

  let allCoreGranted = $state(false);

  let cleanupIntensity = $state<CleanupIntensity>('medium');
  let tone = $state<ToneId>('casual');
  let appearance = $state<AppearanceMode>('system');
  let language = $state<TranscriptionLanguageCode>('en');
  let quickPrefs = $state({
    cleanup: true,
    noise: true,
    caps: true,
    autoSpacing: true,
    capsLock: false,
    appContextHint: false,
    autoLearn: false,
    autostart: false,
    muteAudio: false,
    exclusiveMic: false,
  });

  let providerDisplayName = $derived(providers.find((p) => p.id === provider)?.name ?? '');
  let setupCalibrationCopy = $derived(getSetupCalibrationCopy(language));
  let cleanupName = $derived(cleanupCards.find((c) => c.id === cleanupIntensity)?.name ?? '');
  let toneName = $derived(toneCards.find((t) => t.id === tone)?.name ?? '');
  let appearanceName = $derived(appearanceModes.find((a) => a.id === appearance)?.name ?? 'System');
  let languageLabel = $derived(getTranscriptionLanguageLabel(language));

  onMount(async () => {
    try {
      const [
        savedAppearance, savedLanguage, savedProvider, savedIntensity, savedTone, keyStatus,
        savedCleanup, savedNoise, savedCaps, savedAutoSpacing, savedCapsLock, savedAppContextHint, savedAutoLearn, savedMute, savedAutostart, savedExclusiveMic,
      ] = await Promise.all([
        invoke<AppearanceMode | null>('get_setting', { key: 'appearance_mode' }),
        invoke<TranscriptionLanguageCode | null>('get_setting', { key: 'transcription_language' }),
        invoke<ProviderId | null>('get_setting', { key: 'transcription_provider' }),
        invoke<CleanupIntensity | null>('get_setting', { key: 'cleanup_intensity' }),
        invoke<ToneId | null>('get_setting', { key: 'default_tone' }),
        invoke<Record<ProviderId, boolean> | null>('get_api_key_status'),
        invoke<boolean | null>('get_setting', { key: 'cleanup_enabled' }),
        invoke<boolean | null>('get_setting', { key: 'noise_reduction' }),
        invoke<boolean | null>('get_setting', { key: 'contextual_caps_enabled' }),
        invoke<boolean | null>('get_setting', { key: 'auto_spacing_enabled' }),
        invoke<boolean | null>('get_setting', { key: 'caps_lock_uppercase_enabled' }),
        invoke<boolean | null>('get_setting', { key: 'app_context_hint' }),
        invoke<boolean | null>('get_setting', { key: 'auto_learn_enabled' }),
        invoke<boolean | null>('get_setting', { key: 'mute_audio' }),
        invoke<boolean | null>('get_setting', { key: 'autostart_enabled' }),
        invoke<boolean | null>('get_setting', { key: 'exclusive_mic' }),
      ]);
      if (savedAppearance === 'system' || savedAppearance === 'light' || savedAppearance === 'dark') appearance = savedAppearance;
      if (savedLanguage && transcriptionLanguages.some((o) => o.code === savedLanguage)) language = savedLanguage;
      if (savedProvider && providers.some((p) => p.id === savedProvider)) provider = savedProvider;
      if (savedIntensity && cleanupCards.some((c) => c.id === savedIntensity)) cleanupIntensity = savedIntensity;
      if (savedTone && toneCards.some((t) => t.id === savedTone)) tone = savedTone;
      if (keyStatus) keySaved = !!keyStatus[provider];
      quickPrefs = {
        cleanup: savedCleanup ?? quickPrefs.cleanup,
        noise: savedNoise ?? quickPrefs.noise,
        caps: savedCaps ?? quickPrefs.caps,
        autoSpacing: savedAutoSpacing ?? quickPrefs.autoSpacing,
        capsLock: savedCapsLock ?? quickPrefs.capsLock,
        appContextHint: savedAppContextHint ?? quickPrefs.appContextHint,
        autoLearn: savedAutoLearn ?? quickPrefs.autoLearn,
        autostart: savedAutostart ?? quickPrefs.autostart,
        muteAudio: savedMute ?? quickPrefs.muteAudio,
        exclusiveMic: savedExclusiveMic ?? quickPrefs.exclusiveMic,
      };
    } catch {}
  });

  function delay(ms: number) { return new Promise((r) => setTimeout(r, ms)); }

  async function advance() {
    if (animating) return;
    direction = 'forward';
    animating = true;
    visible = false;
    await delay(220);
    step++;
    visible = true;
    await delay(220);
    animating = false;
  }
  const goNext = advance;
  const skip = advance;

  function jumpToStep(target: number) {
    if (target < step) { direction = 'back'; step = target; }
  }

  async function saveKey() {
    const trimmed = apiKeyDraft.trim();
    if (!trimmed) return;
    keySaving = true;
    keyError = '';
    keyValidation = { status: 'idle', message: '' };
    try {
      await invoke('save_api_key', { provider, key: trimmed });
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
    keyValidation = { status: 'checking', message: '' };
    try {
      const result = await invoke<{ ok: boolean; status: 'valid' | 'invalid' | 'unknown'; message: string }>('validate_api_key', { provider, key });
      keyValidation = { status: result.status, message: result.message };
    } catch {
      keyValidation = { status: 'unknown', message: "Couldn't verify the key right now." };
    }
  }

  async function finish() {
    try {
      await saveSetting('cleanup_intensity', cleanupIntensity);
      await saveSetting('default_tone', tone);
      await saveSetting('transcription_provider', provider);
      await saveSetting('transcription_language', language);
      await saveSetting('cleanup_provider', provider);
      await saveSetting('appearance_mode', appearance);
      await saveSetting('cleanup_enabled', quickPrefs.cleanup);
      await saveSetting('noise_reduction', quickPrefs.noise);
      await saveSetting('contextual_caps_enabled', quickPrefs.caps);
      await saveSetting('auto_spacing_enabled', quickPrefs.autoSpacing);
      await saveSetting('caps_lock_uppercase_enabled', quickPrefs.capsLock);
      await saveSetting('app_context_hint', quickPrefs.appContextHint);
      await saveSetting('auto_learn_enabled', quickPrefs.autoLearn);
      await saveSetting('mute_audio', quickPrefs.muteAudio);
      await saveSetting('exclusive_mic', quickPrefs.exclusiveMic);
      await invoke('set_autostart', { enabled: quickPrefs.autostart });
      await saveSetting('setup_complete', true);
    } catch {}
    appStore.appearanceMode = appearance;
    appStore.setupComplete = true;
  }

  type HeaderInfo = { title: string; subtitle: string; name: string } | null;
  function headerFor(s: number): HeaderInfo {
    if (s === providerStep) return { name: 'Provider', title: 'Choose your AI provider', subtitle: 'This powers both transcription and text cleanup. You can switch anytime in Settings.' };
    if (s === apiKeyStep) return { name: 'API Key', title: `Enter your ${providerDisplayName} API key`, subtitle: 'Keys are stored locally and never leave your machine.' };
    if (isMac && s === permissionStep) return { name: 'Permissions', title: 'Check your macOS permissions', subtitle: 'Verenu needs these to hear your voice and type for you.' };
    if (s === writingStyleStep) return { name: 'Writing Style', title: 'How should your dictation sound?', subtitle: 'Cleanup intensity and tone shape every transcription. You can override both per-app later.' };
    if (s === appearanceStep) return { name: 'Appearance', title: 'Choose your appearance', subtitle: 'Choose your default theme mode. You can change this later in Settings.' };
    if (s === quickSettingsStep) return { name: 'Quick Settings', title: 'A few things worth knowing about', subtitle: 'Defaults that work for most people — change them anytime in Settings.' };
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
      if (keySaving) return bar({ leftLabel: 'Skip for now', rightLabel: 'Saving…', rightDisabled: true, onRight: () => {} });
      if (apiKeyDraft.trim()) return bar({ leftLabel: 'Skip for now', rightLabel: 'Save Key', onRight: saveKey });
      return bar({ leftLabel: 'Skip for now', rightLabel: 'Continue', onRight: goNext });
    }
    if (isMac && step === permissionStep) {
      return bar({
        leftLabel: null,
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
    return bar({ leftLabel: 'Skip for now', rightLabel: 'Next', onRight: goNext });
  });
</script>

<SetupShell
  {step}
  totalSteps={TOTAL_STEPS}
  header={headerFor(step)}
  onDotClick={jumpToStep}
>
  {#snippet left()}
    {#if actionBar.leftLabel}
      <button class="btn-skip" onclick={actionBar.onLeft} disabled={actionBar.leftDisabled || animating}>{actionBar.leftLabel}</button>
    {/if}
  {/snippet}

  {#snippet right()}
    <button
      class="btn-primary"
      class:btn-lg={actionBar.rightLg}
      class:btn-primary--glow={actionBar.rightGlow}
      onclick={actionBar.onRight}
      disabled={actionBar.rightDisabled || animating}
    >{actionBar.rightLabel}</button>
  {/snippet}

  <div class="step-wrap" class:visible class:slide-left={direction === 'forward'} class:slide-right={direction === 'back'}>
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
        {keySaved}
        {keySaving}
        {keyError}
        {keyValidation}
      />
    {:else if isMac && step === permissionStep}
      <PermissionsStep {provider} bind:allCoreGranted />
    {:else if step === writingStyleStep}
      <WritingStyleStep bind:intensity={cleanupIntensity} bind:tone />
    {:else if step === appearanceStep}
      <AppearanceStep bind:appearance />
    {:else if step === quickSettingsStep}
      <QuickSettingsStep bind:quickPrefs bind:language />
    {:else if step === calibrationStep}
      <CalibrationStep {language} />
    {:else if step === tryItStep}
      <TryItStep />
    {:else if step === doneStep}
      <DoneStep
        providerName={providerDisplayName}
        {cleanupName}
        {toneName}
        {languageLabel}
        {appearanceName}
        hasKey={keySaved}
      />
    {/if}
  </div>
</SetupShell>

<style>
  .step-wrap {
    width: 100%;
    display: flex;
    justify-content: center;
    opacity: 0;
    transform: translateX(28px);
    transition: opacity 0.22s ease, transform 0.22s ease;
  }

  .step-wrap.visible { opacity: 1; transform: translateX(0); }
  .step-wrap.slide-right { transform: translateX(-28px); }
  .step-wrap.slide-right.visible { transform: translateX(0); }
</style>
