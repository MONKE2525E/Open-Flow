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
  speakingHint: string;
  noSpeechDetected: string;
  defaultDevice: string;
  noDevicesFound: string;
};

type SetupCopy = {
  title: string;
  subtitle: string;
  startInstruction: string;
  startButton: string;
  readPrompt: string;
  readPhrase: string;
  silenceTitle: string;
  silenceDescription: string;
  successTitle: string;
  successDescription: string;
  successTail: string;
  skipButton: string;
  continueButton: string;
  skipCalibrationButton: string;
  cancelButton: string;
  recalibrateButton: string;
};

const AUDIO_COPY: Record<BaseLanguage, AudioCopy> = {
  en: {
    inputDeviceLabel: 'Input device',
    inputDeviceDescription: 'Choose which microphone Open Flow should record from',
    autoCalibrateButton: 'Auto Calibrate',
    speakingHint: 'Speak: "Open Flow is fast"',
    noSpeechDetected: 'No speech was detected during calibration. Please check your microphone input.',
    defaultDevice: 'Default Device',
    noDevicesFound: 'No devices found',
  },
  es: {
    inputDeviceLabel: 'Dispositivo de entrada',
    inputDeviceDescription: 'Elige qué micrófono debe usar Open Flow para grabar',
    autoCalibrateButton: 'Calibración automática',
    speakingHint: 'Di: "Open Flow es rápido"',
    noSpeechDetected: 'No se detectó voz durante la calibración. Revisa la entrada del micrófono.',
    defaultDevice: 'Dispositivo predeterminado',
    noDevicesFound: 'No se encontraron dispositivos',
  },
  fr: {
    inputDeviceLabel: 'Périphérique d’entrée',
    inputDeviceDescription: 'Choisissez le micro que Open Flow doit utiliser',
    autoCalibrateButton: 'Calibrage auto',
    speakingHint: 'Dites : "Open Flow est rapide"',
    noSpeechDetected: "Aucune voix détectée pendant l’étalonnage. Vérifiez l’entrée micro.",
    defaultDevice: 'Périphérique par défaut',
    noDevicesFound: 'Aucun périphérique trouvé',
  },
  de: {
    inputDeviceLabel: 'Eingabegerät',
    inputDeviceDescription: 'Wähle das Mikrofon aus, das Open Flow verwenden soll',
    autoCalibrateButton: 'Auto-Kalibrierung',
    speakingHint: 'Sprich: "Open Flow ist schnell"',
    noSpeechDetected: 'Während der Kalibrierung wurde keine Sprache erkannt. Prüfe dein Mikrofon.',
    defaultDevice: 'Standardgerät',
    noDevicesFound: 'Keine Geräte gefunden',
  },
  pt: {
    inputDeviceLabel: 'Dispositivo de entrada',
    inputDeviceDescription: 'Escolha qual microfone o Open Flow deve usar',
    autoCalibrateButton: 'Calibração automática',
    speakingHint: 'Fale: "Open Flow é rápido"',
    noSpeechDetected: 'Nenhuma fala foi detectada durante a calibração. Verifique o microfone.',
    defaultDevice: 'Dispositivo padrão',
    noDevicesFound: 'Nenhum dispositivo encontrado',
  },
  zh: {
    inputDeviceLabel: '输入设备',
    inputDeviceDescription: '选择 Open Flow 要使用的麦克风',
    autoCalibrateButton: '自动校准',
    speakingHint: '请说：“Open Flow 很快”',
    noSpeechDetected: '校准期间未检测到语音。请检查麦克风输入。',
    defaultDevice: '默认设备',
    noDevicesFound: '未找到设备',
  },
};

const SETUP_COPY: Record<BaseLanguage, SetupCopy> = {
  en: {
    title: 'Optimize your microphone',
    subtitle: 'We will adjust the gain so the AI can transcribe your voice clearly.',
    startInstruction: 'Click below, then speak naturally for 3 seconds.',
    startButton: 'Start Calibration',
    readPrompt: 'Read this phrase aloud:',
    readPhrase: 'Open Flow makes dictation easy.',
    silenceTitle: 'Silence Detected',
    silenceDescription: "No speech was detected. Make sure your microphone is selected, unmuted, and that you spoke during the countdown. We've defaulted the gain to",
    successTitle: 'Calibration Complete!',
    successDescription: "We've adjusted your microphone gain to",
    successTail: 'Your voice levels are now optimized for transcription.',
    skipButton: 'Skip calibration',
    continueButton: 'Continue',
    skipCalibrationButton: 'Skip Calibration',
    cancelButton: 'Cancel',
    recalibrateButton: 'Recalibrate',
  },
  es: {
    title: 'Optimiza tu micrófono',
    subtitle: 'Ajustaremos la ganancia para que la IA transcriba tu voz con claridad.',
    startInstruction: 'Haz clic abajo y habla con naturalidad durante 3 segundos.',
    startButton: 'Iniciar calibración',
    readPrompt: 'Lee esta frase en voz alta:',
    readPhrase: 'Open Flow facilita el dictado.',
    silenceTitle: 'Se detectó silencio',
    silenceDescription: 'No se detectó voz. Verifica que el micrófono esté seleccionado, activado y que hablaste durante la cuenta regresiva. Dejamos la ganancia en',
    successTitle: '¡Calibración completa!',
    successDescription: 'Ajustamos la ganancia del micrófono a',
    successTail: 'Tus niveles de voz ahora están optimizados para la transcripción.',
    skipButton: 'Omitir calibración',
    continueButton: 'Continuar',
    skipCalibrationButton: 'Omitir calibración',
    cancelButton: 'Cancelar',
    recalibrateButton: 'Recalibrar',
  },
  fr: {
    title: 'Optimisez votre microphone',
    subtitle: "Nous allons ajuster le gain pour que l’IA transcrive clairement votre voix.",
    startInstruction: 'Cliquez ci-dessous puis parlez naturellement pendant 3 secondes.',
    startButton: "Démarrer l’étalonnage",
    readPrompt: 'Lisez cette phrase à voix haute :',
    readPhrase: 'Open Flow facilite la dictée.',
    silenceTitle: 'Silence détecté',
    silenceDescription: "Aucune voix détectée. Vérifiez que le micro est sélectionné, activé et que vous avez parlé pendant le décompte. Le gain a été défini à",
    successTitle: 'Étalonnage terminé !',
    successDescription: 'Nous avons ajusté le gain du microphone à',
    successTail: 'Les niveaux de voix sont maintenant optimisés pour la transcription.',
    skipButton: "Ignorer l’étalonnage",
    continueButton: 'Continuer',
    skipCalibrationButton: "Ignorer l’étalonnage",
    cancelButton: 'Annuler',
    recalibrateButton: 'Recalibrer',
  },
  de: {
    title: 'Mikrofon optimieren',
    subtitle: 'Wir passen die Verstärkung an, damit die KI deine Stimme klar transkribiert.',
    startInstruction: 'Klicke unten und sprich dann 3 Sekunden lang natürlich.',
    startButton: 'Kalibrierung starten',
    readPrompt: 'Lies diesen Satz laut vor:',
    readPhrase: 'Open Flow macht Diktieren einfach.',
    silenceTitle: 'Stille erkannt',
    silenceDescription: 'Es wurde keine Sprache erkannt. Prüfe Mikrofonwahl, Stummschaltung und ob du während des Countdowns gesprochen hast. Die Verstärkung wurde gesetzt auf',
    successTitle: 'Kalibrierung abgeschlossen!',
    successDescription: 'Wir haben die Mikrofonverstärkung angepasst auf',
    successTail: 'Deine Sprachpegel sind jetzt für die Transkription optimiert.',
    skipButton: 'Kalibrierung überspringen',
    continueButton: 'Weiter',
    skipCalibrationButton: 'Kalibrierung überspringen',
    cancelButton: 'Abbrechen',
    recalibrateButton: 'Neu kalibrieren',
  },
  pt: {
    title: 'Otimize seu microfone',
    subtitle: 'Vamos ajustar o ganho para que a IA transcreva sua voz com clareza.',
    startInstruction: 'Clique abaixo e fale naturalmente por 3 segundos.',
    startButton: 'Iniciar calibração',
    readPrompt: 'Leia esta frase em voz alta:',
    readPhrase: 'Open Flow facilita o ditado.',
    silenceTitle: 'Silêncio detectado',
    silenceDescription: 'Nenhuma fala foi detectada. Verifique se o microfone está selecionado, sem mudo e se você falou durante a contagem. Definimos o ganho para',
    successTitle: 'Calibração concluída!',
    successDescription: 'Ajustamos o ganho do microfone para',
    successTail: 'Seus níveis de voz agora estão otimizados para transcrição.',
    skipButton: 'Pular calibração',
    continueButton: 'Continuar',
    skipCalibrationButton: 'Pular calibração',
    cancelButton: 'Cancelar',
    recalibrateButton: 'Recalibrar',
  },
  zh: {
    title: '优化你的麦克风',
    subtitle: '我们会调整增益，让 AI 更清晰地转写你的语音。',
    startInstruction: '点击下方按鈕，然后自然说话 3 秒。',
    startButton: '开始校准',
    readPrompt: '请大声朗读这句话：',
    readPhrase: 'Open Flow 让语音输入更轻松。',
    silenceTitle: '检测到静音',
    silenceDescription: '未检测到语音。请确认麦克风已选择、未静音，并在倒计时期间说话。我们已将增益设置为',
    successTitle: '校准完成！',
    successDescription: '我们已将麦克风增益调整为',
    successTail: '你的语音电平现在已针对转写进行优化。',
    skipButton: '跳过校准',
    continueButton: '继续',
    skipCalibrationButton: '跳过校准',
    cancelButton: '取消',
    recalibrateButton: '重新校准',
  },
};

export function getAudioCalibrationCopy(languageCode: TranscriptionLanguageCode | string): AudioCopy {
  return AUDIO_COPY[baseLanguage(languageCode)];
}

export function getSetupCalibrationCopy(languageCode: TranscriptionLanguageCode | string): SetupCopy {
  return SETUP_COPY[baseLanguage(languageCode)];
}
