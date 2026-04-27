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
  contextualStrings?: string[];
  maxAlternatives?: number;
};
