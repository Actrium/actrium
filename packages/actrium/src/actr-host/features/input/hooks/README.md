# Input Hooks Annotations

This document only describes two related to voice input controller hookand current `ActrInput` The access programme.

## 1. `useActrVoiceController`

Documents:[`useActrVoiceController.ts`](/Users/toupitutuetoudada/Desktop/work/github/ipad/actro-runtime/src/actr-host/features/input/hooks/useActrVoiceController.ts)

Duties:

- Process“Hold on to send voicemails.”Full link
- Manage audio start, slice cache, threshold submission, remote voice session opening and closing
- Discard local voice files after sending finished and re-route them through messages

Elements for achievement:

- Pass. `useAudioRecordingStream` Keep getting audio slices.
- We'll get into the local buffer after the recording starts. 1 Only after the second threshold will you really create voice messages and start a remote voice session
- When the release is over, wait for the slice to be sent and then call. `endVoice`
- When you slide off or release before the threshold, you will discard the current recording.
- Voice message-related session ability is not in the right place. hook Internal generation, but relying on `useActrMessageFlow` Organisation
  - `processVoiceMessageSend`
  - `sendVoiceChunk`
  - `startVoice`
  - `endVoice`
  - `onVoiceMessageReady`
  - `onVoiceMessageCanceled`

Applied scene:

- The user voice needs to be used as a tool for“Voice message”Send directly, not when text is converted

## 2. `useActrSpeechRecognitionController`

Documents:[`useActrSpeechRecognitionController.ts`](/Users/toupitutuetoudada/Desktop/work/github/ipad/actro-runtime/src/actr-host/features/input/hooks/useActrSpeechRecognitionController.ts)

Duties:

- Process“Hold on to voice recognition, and then release and send by text.”The link.
- Management of the start, cancellation, cessation and, ultimately, transcript ♪ When you send ♪

Elements for achievement:

- Based on `SpeechRecognitionService` Start and stop identification
- Cache only during recognition transcript, do not write text directly to the input box
- If you don't cancel, you'll have to. `SpeechRecognitionService` Read Current transcript As a back pocket, call again. `stop`
- Waiting for recognition service to trigger `onEnd` , and then call in unison `processMessageSend` Send Text Message
- Call on Cancel `abort`, do not send any messages

Applied scene:

- Retention required“Hold on to your word.”. The interactive, but ultimately text messages

## 3. Current access scheme

Current `ActrInput` What's used is `useActrSpeechRecognitionController`.

Access location:

- Documents:[`ActrInput.shared.tsx`](/Users/toupitutuetoudada/Desktop/work/github/ipad/actro-runtime/src/actr-host/features/input/ActrInput.shared.tsx)

Current act(s):

- Phone and Pad The voice buttons, long hand gesture entrances are still in place. UI Structure
- But it's coming up the top. `handleVoiceStart / handleVoiceEnd / handleVoiceChangeState` It's cut. `useActrSpeechRecognitionController`
- The current practice is therefore:
  - Hold on to start voice recognition.
  - Slip Cancel Not Send
  - End identification when you release.
  - The identified text goes straight. `processMessageSend` Send

## 4. If we want to resume original voicemail transmission

The recovery point is mainly in the [`ActrInput.shared.tsx`](/Users/toupitutuetoudada/Desktop/work/github/ipad/actro-runtime/src/actr-host/features/input/ActrInput.shared.tsx):

- The one that's currently connected to the `useActrSpeechRecognitionController` Change back. `useActrVoiceController`
- And re-do it. `useActrMessageFlow` Returning voice sessions pass back

Phone / Pad The view layers are usually not rewritten because they are mainly hand signals entry points, and the real voice logic is that they are the only ones that have a voice. controller hook Lee.
