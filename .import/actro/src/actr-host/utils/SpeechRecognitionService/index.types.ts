export type Unsubscribe = () => void;

export type SpeechRecognitionResultItem = {
  transcript: string;
  confidence: number;
  segments: Array<{
    startTimeMillis: number;
    endTimeMillis: number;
    segment: string;
    confidence: number;
  }>;
};

export type SpeechRecognitionErrorEvent = {
  error: string;
  message: string;
  code?: number;
};

export type SpeechRecognitionState =
  | 'idle'
  | 'authorizing'
  | 'recognizing'
  | 'stopping'
  | 'error';

export type SpeechRecognitionTranscriptEvent = {
  text: string;
  isFinal: boolean;
  results: SpeechRecognitionResultItem[];
};

export type SpeechRecognitionCallbacks = {
  onStart?: () => void;
  onEnd?: () => void;
  /** 中文注释：语音识别期间的 0 到 1 实时音量值，用于录音浮层波形反馈。 */
  onMetering?: (metering: number) => void;
  onPartialResult?: (event: SpeechRecognitionTranscriptEvent) => void;
  onFinalResult?: (event: SpeechRecognitionTranscriptEvent) => void;
  onError?: (error: SpeechRecognitionErrorEvent) => void;
};

export type StartSpeechRecognitionOptions = {
  lang?: string;
  continuous?: boolean;
  interimResults?: boolean;
  requiresOnDeviceRecognition?: boolean;
  addsPunctuation?: boolean;
  /** 中文注释：开启原生音量事件，用于录音浮层实时波形。 */
  volumeChangeEventOptions?: {
    enabled?: boolean;
    intervalMillis?: number;
  };
  contextualStrings?: string[];
  maxAlternatives?: number;
};
