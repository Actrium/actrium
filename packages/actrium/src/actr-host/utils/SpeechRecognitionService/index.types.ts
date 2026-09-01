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
  /** Note: during speech recognition 0 Present. 1 Real-time volume values for sounding float-wave feedback. */
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
  /** Note: Opens a native volume event for recording a real-time waveform of the float. */
  volumeChangeEventOptions?: {
    enabled?: boolean;
    intervalMillis?: number;
  };
  contextualStrings?: string[];
  maxAlternatives?: number;
};
