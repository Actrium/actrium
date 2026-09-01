import { useCallback, useEffect, useRef, useState } from 'react';
import { useEventBridge } from 'askit';
import {
  Button,
  ScrollView,
  Text,
  TextInput,
  View,
} from 'keel/guest';

import { decodeEchoResponse, encodeEchoRequest } from './protobuf';

type ActrCallRequest = {
  requestId: string;
  routeKey: string;
  payloadBase64: string;
  timeoutMs: number;
};

type ActrCallResponse = {
  requestId?: string;
  success: boolean;
  payloadBase64?: string;
  error?: string;
};

type EchoHistoryItem = {
  request: string;
  response: string;
  timestamp: number;
};

export default function EchoAtActrium() {
  const initialProbeSent = useRef(false);
  const [message, setMessage] = useState('Hello from Actrium');
  const [isSending, setIsSending] = useState(false);
  const [error, setError] = useState('');
  const [history, setHistory] = useState<EchoHistoryItem[]>([]);
  const { request: callActor } = useEventBridge<
    ActrCallRequest,
    ActrCallResponse
  >('ACTR_CALL', 'ACTR_CALL_RESULT', { timeoutMs: 35_000 });

  const send = useCallback(async () => {
    const trimmed = message.trim();
    if (!trimmed || isSending) return;

    setIsSending(true);
    setError('');
    try {
      console.log(
        '[EchoAtActrium] sending ACTR_CALL',
        typeof callActor,
      );
      const result = await callActor({
        requestId: `echo-${Date.now()}`,
        routeKey: 'echo.EchoService.Echo',
        payloadBase64: encodeEchoRequest(trimmed),
        timeoutMs: 30_000,
      });
      if (!result.success || !result.payloadBase64) {
        throw new Error(result.error || 'Actor call failed');
      }

      const response = decodeEchoResponse(result.payloadBase64);
      console.log('[EchoAtActrium] received echo response');
      setHistory((items) => [
        { request: trimmed, response: response.reply, timestamp: response.timestamp },
        ...items,
      ]);
    } catch (cause) {
      console.error('[EchoAtActrium] ACTR_CALL failed', cause);
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setIsSending(false);
    }
  }, [callActor, isSending, message]);

  useEffect(() => {
    if (initialProbeSent.current) return;
    initialProbeSent.current = true;
    void send();
  }, [send]);

  return (
    <ScrollView
      style={{ flex: 1, backgroundColor: '#101827' }}
      contentContainerStyle={{ padding: 32, alignItems: 'center' }}
    >
      <View style={{ width: '100%', maxWidth: 720 }}>
        <Text style={{ color: '#f8fafc', fontSize: 32, fontWeight: '700' }}>
          Echo at Actrium
        </Text>
        <Text style={{ color: '#94a3b8', fontSize: 15, marginTop: 8 }}>
          Keel Guest → Actrium Host → Loran → Echo Actor
        </Text>

        <View
          style={{
            backgroundColor: '#17233a',
            borderColor: '#2c4165',
            borderWidth: 1,
            borderRadius: 16,
            padding: 20,
            marginTop: 28,
          }}
        >
          <TextInput
            value={message}
            onChangeText={setMessage}
            editable={!isSending}
            placeholder='Message to echo'
            placeholderTextColor='#64748b'
            style={{
              color: '#f8fafc',
              backgroundColor: '#0f172a',
              borderColor: '#334155',
              borderWidth: 1,
              borderRadius: 10,
              fontSize: 17,
              padding: 14,
            }}
          />
          <View style={{ marginTop: 14 }}>
            <Button
              title={isSending ? 'Sending…' : 'Send to Actor'}
              onPress={send}
              disabled={isSending || !message.trim()}
              color='#2563eb'
            />
          </View>
          {error ? (
            <Text style={{ color: '#fda4af', fontSize: 14, marginTop: 14 }}>
              {error}
            </Text>
          ) : null}
        </View>

        <Text
          style={{
            color: '#cbd5e1',
            fontSize: 17,
            fontWeight: '600',
            marginTop: 28,
            marginBottom: 10,
          }}
        >
          Replies
        </Text>
        {history.length === 0 ? (
          <Text style={{ color: '#64748b', fontSize: 14 }}>
            Send a message to exercise the complete Actor path.
          </Text>
        ) : (
          history.map((item, index) => (
            <View
              key={`${item.timestamp}-${index}`}
              style={{
                backgroundColor: '#172033',
                borderRadius: 12,
                marginBottom: 10,
                padding: 16,
              }}
            >
              <Text style={{ color: '#94a3b8', fontSize: 13 }}>
                {item.request}
              </Text>
              <Text style={{ color: '#dbeafe', fontSize: 17, marginTop: 6 }}>
                {item.response}
              </Text>
            </View>
          ))
        )}
      </View>
    </ScrollView>
  );
}
