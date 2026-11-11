import { useCallback, useState } from 'react';
import { ScrollView, Text, TextInput, View } from 'keel/guest';

type Message = {
  id: number;
  role: 'assistant' | 'user';
  text: string;
};

const INITIAL_MESSAGES: Message[] = [
  {
    id: 1,
    role: 'assistant',
    text: '你好，我是 Actrium 助手。有什么可以帮你？',
  },
  {
    id: 2,
    role: 'user',
    text: '帮我看看今天的安排。',
  },
  {
    id: 3,
    role: 'assistant',
    text: '今天下午有一个产品评审，其余时间空闲。',
  },
  {
    id: 4,
    role: 'user',
    text: '帮我把评审材料整理一下。',
  },
  {
    id: 5,
    role: 'assistant',
    text: '好的，已经列出大纲：目标、范围、风险、验收标准。',
  },
  {
    id: 6,
    role: 'user',
    text: '再补一个时间线。',
  },
  {
    id: 7,
    role: 'assistant',
    text: '建议按 30 分钟评审、15 分钟讨论、15 分钟结论来安排。',
  },
  {
    id: 8,
    role: 'user',
    text: '可以，顺便提醒我提前十分钟。',
  },
  {
    id: 9,
    role: 'assistant',
    text: '已记录，会在评审前十分钟提醒你。',
  },
  {
    id: 10,
    role: 'user',
    text: '评审材料里再补一个竞品对比。',
  },
  {
    id: 11,
    role: 'assistant',
    text: '好的，我会按功能、价格、生态三个维度整理竞品对比。',
  },
  {
    id: 12,
    role: 'user',
    text: '重点看一下移动端体验。',
  },
  {
    id: 13,
    role: 'assistant',
    text: '移动端会单独列一页，覆盖启动速度、输入体验和离线能力。',
  },
  {
    id: 14,
    role: 'user',
    text: '把风险项也列出来。',
  },
  {
    id: 15,
    role: 'assistant',
    text: '风险项包括：范围蔓延、接口变更、性能回退和测试覆盖不足。',
  },
  {
    id: 16,
    role: 'user',
    text: '每个风险给一个缓解方案。',
  },
  {
    id: 17,
    role: 'assistant',
    text: '已补充：冻结范围、接口版本化、性能基线和自动化回归。',
  },
  {
    id: 18,
    role: 'user',
    text: '好，先这样。',
  },
]

export default function DialogApp() {
  const [messages, setMessages] = useState<Message[]>(INITIAL_MESSAGES);
  const [input, setInput] = useState('');

  const send = useCallback(() => {
    const text = input.trim();
    if (!text) return;

    setMessages((prev) => [
      ...prev,
      { id: Date.now(), role: 'user', text },
    ]);
    setInput('');
  }, [input]);

  return (
    <View style={{ flex: 1, backgroundColor: '#f5f7fa' }}>
      <View
        style={{
          paddingHorizontal: 20,
          paddingVertical: 14,
          backgroundColor: '#ffffff',
          borderBottomWidth: 1,
          borderBottomColor: '#e5e7eb',
        }}
      >
        <Text style={{ fontSize: 18, fontWeight: '600', color: '#111827' }}>
          Actrium 对话
        </Text>
      </View>

      <ScrollView
        style={{ flex: 1 }}
        contentContainerStyle={{ padding: 16 }}
        keyboardShouldPersistTaps="handled"
      >
        {messages.map((message) => {
          const isUser = message.role === 'user';
          return (
            <View
              key={message.id}
              style={{
                alignSelf: isUser ? 'flex-end' : 'flex-start',
                maxWidth: '80%',
                marginBottom: 10,
                paddingHorizontal: 12,
                paddingVertical: 10,
                borderRadius: 12,
                backgroundColor: isUser ? '#2563eb' : '#ffffff',
                borderWidth: isUser ? 0 : 1,
                borderColor: '#e5e7eb',
              }}
            >
              <Text
                style={{
                  fontSize: 15,
                  lineHeight: 21,
                  color: isUser ? '#ffffff' : '#1f2937',
                }}
              >
                {message.text}
              </Text>
            </View>
          );
        })}
      </ScrollView>

      <View
        style={{
          paddingHorizontal: 16,
          paddingTop: 8,
          paddingBottom: 14,
          backgroundColor: '#f5f7fa',
        }}
      >
        <View
          style={{
            width: '100%',
            minHeight: 96,
            backgroundColor: '#ffffff',
            borderWidth: 1,
            borderColor: '#d1d5db',
            borderRadius: 18,
            paddingHorizontal: 16,
            paddingTop: 12,
            paddingBottom: 52,
            overflow: 'hidden',
            shadowColor: '#000000',
            shadowOffset: { width: 0, height: 2 },
            shadowOpacity: 0.08,
            shadowRadius: 8,
          }}
        >
          <TextInput
            value={input}
            onChangeText={setInput}
            placeholder="输入消息..."
            placeholderTextColor="#9ca3af"
            multiline
            numberOfLines={2}
            returnKeyType="send"
            onSubmitEditing={send}
            style={{
              width: '100%',
              minHeight: 42,
              padding: 0,
              paddingRight: 0,
              backgroundColor: 'transparent',
              color: '#111827',
              fontSize: 15,
              lineHeight: 21,
            }}
          />
          <Text
            onPress={input.trim() ? send : undefined}
            style={{
              position: 'absolute',
              right: 12,
              bottom: 12,
              width: 30,
              height: 30,
              lineHeight: 30,
              borderRadius: 15,
              backgroundColor: input.trim() ? '#2563eb' : '#cbd5e1',
              color: '#ffffff',
              fontSize: 18,
              fontWeight: '600',
              textAlign: 'center',
            }}
          >
            ↑
          </Text>
        </View>
      </View>
    </View>
  );
}
