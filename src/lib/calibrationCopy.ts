import type { TranscriptionLanguageCode } from './transcriptionLanguages';

type BaseLanguage = 'en' | 'es' | 'fr' | 'de' | 'pt' | 'zh';

function baseLanguage(code: TranscriptionLanguageCode | string): BaseLanguage {
  const base = String(code || 'en').toLowerCase().split('-')[0];
  if (base === 'es' || base === 'fr' || base === 'de' || base === 'pt' || base === 'zh') {
    return base;
  }
  return 'en';
}

type AudioCopy = {
  inputDeviceLabel: string;
  inputDeviceDescription: string;
  autoCalibrateButton: string;
  quietHint: string;
  speakingHint: string;
  whisperHint: string;
  phase1Label: string;
  phase2Label: string;
  phase3Label: string;
  noSpeechDetected: string;
  defaultDevice: string;
  noDevicesFound: string;
};

type SetupCopy = {
  title: string;
  subtitle: string;
  /** Shown before starting — the room matters as much as the mic. */
  quietHint: string;
  step0Text: string;
  step1Text: string;
  step2Text: string;
  startButton: string;
  ambientPrompt: string;
  ambientPhrase: string;
  readPrompt: string;
  readPhrase: string;
  whisperPrompt: string;
  whisperPhrase: string;
  phase1Label: string;
  phase2Label: string;
  phase3Label: string;
  silenceTitle: string;
  silenceDescription: string;
  successTitle: string;
  successDescription: string;
  successTail: string;
  /** Appended to the result when the ambient phase heard a loud room. */
  noisyRoomNote: string;
  skipButton: string;
  continueButton: string;
  cancelButton: string;
  recalibrateButton: string;
};

const AUDIO_COPY: Record<BaseLanguage, AudioCopy> = {
  en: {
    inputDeviceLabel: 'Input device',
    inputDeviceDescription: 'Choose which microphone Verenu should record from',
    autoCalibrateButton: 'Auto Calibrate',
    quietHint: 'Stay silent',
    speakingHint: 'Speak: "Verenu is fast"',
    whisperHint: 'Whisper something',
    phase1Label: '1/3',
    phase2Label: '2/3',
    phase3Label: '3/3',
    noSpeechDetected: 'No speech was detected during calibration. Please check your microphone input.',
    defaultDevice: 'Default Device',
    noDevicesFound: 'No devices found',
  },
  es: {
    inputDeviceLabel: 'Dispositivo de entrada',
    inputDeviceDescription: 'Elige qué micrófono debe usar Verenu para grabar',
    autoCalibrateButton: 'Calibración automática',
    quietHint: 'Guarda silencio',
    speakingHint: 'Di: "Verenu es rápido"',
    whisperHint: 'Susurra algo',
    phase1Label: '1/3',
    phase2Label: '2/3',
    phase3Label: '3/3',
    noSpeechDetected: 'No se detectó voz durante la calibración. Revisa la entrada del micrófono.',
    defaultDevice: 'Dispositivo predeterminado',
    noDevicesFound: 'No se encontraron dispositivos',
  },
  fr: {
    inputDeviceLabel: "Périphérique d'entrée",
    inputDeviceDescription: 'Choisissez le micro que Verenu doit utiliser',
    autoCalibrateButton: 'Calibrage auto',
    quietHint: 'Restez silencieux',
    speakingHint: 'Dites : "Verenu est rapide"',
    whisperHint: 'Chuchotez quelque chose',
    phase1Label: '1/3',
    phase2Label: '2/3',
    phase3Label: '3/3',
    noSpeechDetected: "Aucune voix détectée pendant l'étalonnage. Vérifiez l'entrée micro.",
    defaultDevice: 'Périphérique par défaut',
    noDevicesFound: 'Aucun périphérique trouvé',
  },
  de: {
    inputDeviceLabel: 'Eingabegerät',
    inputDeviceDescription: 'Wähle das Mikrofon aus, das Verenu verwenden soll',
    autoCalibrateButton: 'Auto-Kalibrierung',
    quietHint: 'Bleib still',
    speakingHint: 'Sprich: "Verenu ist schnell"',
    whisperHint: 'Flüstere etwas',
    phase1Label: '1/3',
    phase2Label: '2/3',
    phase3Label: '3/3',
    noSpeechDetected: 'Während der Kalibrierung wurde keine Sprache erkannt. Prüfe dein Mikrofon.',
    defaultDevice: 'Standardgerät',
    noDevicesFound: 'Keine Geräte gefunden',
  },
  pt: {
    inputDeviceLabel: 'Dispositivo de entrada',
    inputDeviceDescription: 'Escolha qual microfone o Verenu deve usar',
    autoCalibrateButton: 'Calibração automática',
    quietHint: 'Fique em silêncio',
    speakingHint: 'Fale: "Verenu é rápido"',
    whisperHint: 'Sussurre algo',
    phase1Label: '1/3',
    phase2Label: '2/3',
    phase3Label: '3/3',
    noSpeechDetected: 'Nenhuma fala foi detectada durante a calibração. Verifique o microfone.',
    defaultDevice: 'Dispositivo padrão',
    noDevicesFound: 'Nenhum dispositivo encontrado',
  },
  zh: {
    inputDeviceLabel: '输入设备',
    inputDeviceDescription: '选择 Verenu 要使用的麦克风',
    autoCalibrateButton: '自动校准',
    quietHint: '保持安静',
    speakingHint: '请说："Verenu 很快"',
    whisperHint: '轻声说些什么',
    phase1Label: '1/3',
    phase2Label: '2/3',
    phase3Label: '3/3',
    noSpeechDetected: '校准期间未检测到语音。请检查麦克风输入。',
    defaultDevice: '默认设备',
    noDevicesFound: '未找到设备',
  },
};

const SETUP_COPY: Record<BaseLanguage, SetupCopy> = {
  en: {
    title: 'Tune your microphone',
    subtitle: 'Three short readings — silence, normal speech, a whisper — set the gain so quiet words still get transcribed.',
    quietHint: 'Find a quiet spot first. Fans, music and traffic all end up in the reading.',
    step0Text: 'Stay silent for 2 seconds',
    step1Text: 'Speak normally for 3 seconds',
    step2Text: 'Whisper for 2 seconds',
    startButton: 'Start Calibration',
    ambientPrompt: 'Stay quiet — measuring your room:',
    ambientPhrase: 'Say nothing',
    readPrompt: 'Read this phrase aloud:',
    readPhrase: 'Verenu makes dictation easy.',
    whisperPrompt: 'Now whisper:',
    whisperPhrase: 'Verenu',
    phase1Label: 'Step 1 of 3',
    phase2Label: 'Step 2 of 3',
    phase3Label: 'Step 3 of 3',
    silenceTitle: 'No speech detected',
    silenceDescription: "We didn't hear a voice. Check that the right microphone is selected and unmuted, and that you spoke during the countdown. Gain is left at",
    successTitle: 'Microphone tuned',
    successDescription: "We've adjusted your microphone gain to",
    successTail: 'Quiet speech should now come through as clearly as normal speech.',
    noisyRoomNote: 'Your room was noisy during the silent step, so this reading is conservative. Recalibrate somewhere quieter for a better result.',
    skipButton: 'Skip calibration',
    continueButton: 'Continue',
    cancelButton: 'Cancel',
    recalibrateButton: 'Recalibrate',
  },
  es: {
    title: 'Ajusta tu micrófono',
    subtitle: 'Tres lecturas cortas —silencio, voz normal, un susurro— fijan la ganancia para que las palabras suaves también se transcriban.',
    quietHint: 'Busca un lugar tranquilo. Los ventiladores, la música y el tráfico acaban en la medición.',
    step0Text: 'Guarda silencio durante 2 segundos',
    step1Text: 'Habla con naturalidad durante 3 segundos',
    step2Text: 'Susurra durante 2 segundos',
    startButton: 'Iniciar calibración',
    ambientPrompt: 'Guarda silencio: estamos midiendo tu sala.',
    ambientPhrase: 'No digas nada',
    readPrompt: 'Lee esta frase en voz alta:',
    readPhrase: 'Verenu facilita el dictado.',
    whisperPrompt: 'Ahora susurra:',
    whisperPhrase: 'Verenu',
    phase1Label: 'Paso 1 de 3',
    phase2Label: 'Paso 2 de 3',
    phase3Label: 'Paso 3 de 3',
    silenceTitle: 'No se detectó voz',
    silenceDescription: 'No escuchamos ninguna voz. Verifica que el micrófono correcto esté seleccionado y activado, y que hablaste durante la cuenta regresiva. La ganancia queda en',
    successTitle: 'Micrófono ajustado',
    successDescription: 'Ajustamos la ganancia del micrófono a',
    successTail: 'El habla suave ahora debería oírse tan claro como la voz normal.',
    noisyRoomNote: 'Tu sala tenía ruido durante el paso en silencio, así que esta medición es conservadora. Recalibra en un lugar más tranquilo para mejorarla.',
    skipButton: 'Omitir calibración',
    continueButton: 'Continuar',
    cancelButton: 'Cancelar',
    recalibrateButton: 'Recalibrar',
  },
  fr: {
    title: 'Réglez votre microphone',
    subtitle: 'Trois courtes mesures — silence, voix normale, chuchotement — fixent le gain pour que les mots discrets soient aussi transcrits.',
    quietHint: 'Installez-vous au calme. Ventilateurs, musique et circulation finissent tous dans la mesure.',
    step0Text: 'Restez silencieux pendant 2 secondes',
    step1Text: 'Parlez naturellement pendant 3 secondes',
    step2Text: 'Chuchotez pendant 2 secondes',
    startButton: "Démarrer l'étalonnage",
    ambientPrompt: 'Restez silencieux — nous mesurons votre pièce :',
    ambientPhrase: 'Ne dites rien',
    readPrompt: 'Lisez cette phrase à voix haute :',
    readPhrase: 'Verenu facilite la dictée.',
    whisperPrompt: 'Maintenant chuchotez :',
    whisperPhrase: 'Verenu',
    phase1Label: 'Étape 1 sur 3',
    phase2Label: 'Étape 2 sur 3',
    phase3Label: 'Étape 3 sur 3',
    silenceTitle: 'Aucune voix détectée',
    silenceDescription: "Nous n'avons entendu aucune voix. Vérifiez que le bon micro est sélectionné et activé, et que vous avez parlé pendant le décompte. Le gain reste à",
    successTitle: 'Microphone réglé',
    successDescription: 'Nous avons ajusté le gain du microphone à',
    successTail: 'La parole discrète devrait maintenant passer aussi clairement que la voix normale.',
    noisyRoomNote: "Votre pièce était bruyante pendant l'étape silencieuse, cette mesure est donc prudente. Recalibrez dans un endroit plus calme pour un meilleur résultat.",
    skipButton: "Ignorer l'étalonnage",
    continueButton: 'Continuer',
    cancelButton: 'Annuler',
    recalibrateButton: 'Recalibrer',
  },
  de: {
    title: 'Mikrofon einstellen',
    subtitle: 'Drei kurze Messungen — Stille, normales Sprechen, Flüstern — setzen die Verstärkung so, dass auch leise Wörter transkribiert werden.',
    quietHint: 'Such dir zuerst einen ruhigen Ort. Lüfter, Musik und Verkehr landen alle in der Messung.',
    step0Text: '2 Sekunden still bleiben',
    step1Text: '3 Sekunden normal sprechen',
    step2Text: '2 Sekunden flüstern',
    startButton: 'Kalibrierung starten',
    ambientPrompt: 'Bleib still — wir messen deinen Raum:',
    ambientPhrase: 'Sag nichts',
    readPrompt: 'Lies diesen Satz laut vor:',
    readPhrase: 'Verenu macht Diktieren einfach.',
    whisperPrompt: 'Jetzt flüstern:',
    whisperPhrase: 'Verenu',
    phase1Label: 'Schritt 1 von 3',
    phase2Label: 'Schritt 2 von 3',
    phase3Label: 'Schritt 3 von 3',
    silenceTitle: 'Keine Sprache erkannt',
    silenceDescription: 'Wir haben keine Stimme gehört. Prüfe, ob das richtige Mikrofon ausgewählt und nicht stummgeschaltet ist und ob du während des Countdowns gesprochen hast. Die Verstärkung bleibt bei',
    successTitle: 'Mikrofon eingestellt',
    successDescription: 'Wir haben die Mikrofonverstärkung angepasst auf',
    successTail: 'Leise Sprache sollte jetzt genauso klar ankommen wie normales Sprechen.',
    noisyRoomNote: 'Dein Raum war während des stillen Schritts laut, daher ist diese Messung konservativ. Kalibriere an einem ruhigeren Ort neu für ein besseres Ergebnis.',
    skipButton: 'Kalibrierung überspringen',
    continueButton: 'Weiter',
    cancelButton: 'Abbrechen',
    recalibrateButton: 'Neu kalibrieren',
  },
  pt: {
    title: 'Ajuste seu microfone',
    subtitle: 'Três medições curtas — silêncio, fala normal, um sussurro — definem o ganho para que palavras baixas também sejam transcritas.',
    quietHint: 'Procure um lugar silencioso primeiro. Ventiladores, música e trânsito entram todos na medição.',
    step0Text: 'Fique em silêncio por 2 segundos',
    step1Text: 'Fale normalmente por 3 segundos',
    step2Text: 'Sussurre por 2 segundos',
    startButton: 'Iniciar calibração',
    ambientPrompt: 'Fique em silêncio — estamos medindo sua sala:',
    ambientPhrase: 'Não diga nada',
    readPrompt: 'Leia esta frase em voz alta:',
    readPhrase: 'Verenu facilita o ditado.',
    whisperPrompt: 'Agora sussurre:',
    whisperPhrase: 'Verenu',
    phase1Label: 'Passo 1 de 3',
    phase2Label: 'Passo 2 de 3',
    phase3Label: 'Passo 3 de 3',
    silenceTitle: 'Nenhuma fala detectada',
    silenceDescription: 'Não ouvimos nenhuma voz. Verifique se o microfone certo está selecionado e sem mudo, e se você falou durante a contagem. O ganho fica em',
    successTitle: 'Microfone ajustado',
    successDescription: 'Ajustamos o ganho do microfone para',
    successTail: 'A fala baixa agora deve chegar tão clara quanto a fala normal.',
    noisyRoomNote: 'Sua sala estava barulhenta durante o passo em silêncio, então esta medição é conservadora. Recalibre em um lugar mais silencioso para um resultado melhor.',
    skipButton: 'Pular calibração',
    continueButton: 'Continuar',
    cancelButton: 'Cancelar',
    recalibrateButton: 'Recalibrar',
  },
  zh: {
    title: '调整你的麦克风',
    subtitle: '三段简短的采样——静音、正常说话、轻声说话——用来设定增益，让轻声的词也能被转写。',
    quietHint: '请先找一个安静的地方。风扇、音乐和车流都会被一起采集进来。',
    step0Text: '保持安静 2 秒',
    step1Text: '正常说话 3 秒',
    step2Text: '轻声说话 2 秒',
    startButton: '开始校准',
    ambientPrompt: '请保持安静——正在测量你的房间：',
    ambientPhrase: '什么都不要说',
    readPrompt: '请大声朗读这句话：',
    readPhrase: 'Verenu 让语音输入更轻松。',
    whisperPrompt: '现在轻声说：',
    whisperPhrase: 'Verenu',
    phase1Label: '第 1 步，共 3 步',
    phase2Label: '第 2 步，共 3 步',
    phase3Label: '第 3 步，共 3 步',
    silenceTitle: '未检测到语音',
    silenceDescription: '我们没有听到人声。请确认已选择正确的麦克风且未静音，并在倒计时期间说话。增益保持为',
    successTitle: '麦克风已调整',
    successDescription: '我们已将麦克风增益调整为',
    successTail: '现在轻声说话应该和正常说话一样清晰。',
    noisyRoomNote: '静音步骤中你的房间比较嘈杂，因此这次测量偏保守。换一个更安静的地方重新校准可以得到更好的结果。',
    skipButton: '跳过校准',
    continueButton: '继续',
    cancelButton: '取消',
    recalibrateButton: '重新校准',
  },
};

/**
 * The interface language, which is English everywhere else in the app.
 *
 * These getters used to switch on `transcription_language` — the language the
 * user *speaks*, not the one they read. Picking Chinese as your dictation
 * language turned parts of the setup wizard and the audio settings Chinese
 * while every other string stayed English. Those are different settings, and
 * only one of them exists today.
 *
 * The translated tables below are kept, unreachable, for whenever a real
 * UI-language setting lands — at which point this is where it plugs in.
 */
const UI_LANGUAGE: BaseLanguage = 'en';

export function getAudioCalibrationCopy(): AudioCopy {
  return AUDIO_COPY[UI_LANGUAGE];
}

export function getSetupCalibrationCopy(): SetupCopy {
  return SETUP_COPY[UI_LANGUAGE];
}
