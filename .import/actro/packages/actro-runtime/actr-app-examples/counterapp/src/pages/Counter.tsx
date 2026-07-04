import { useState } from 'react';
import { ScrollView, View, Text, TouchableOpacity } from 'rill/let';
// import Geo from '../components/Geo';
import Speech from '../components/Speech';
import Weather from '../components/Weather';

interface CounterProps {
  onChange?: (action: string, emoji: string, color: string) => void;
}

export default function Counter({ onChange }: CounterProps) {
  const [redCount, setRedCount] = useState(0);
  const [blueCount, setBlueCount] = useState(0);
  const [greenCount, setGreenCount] = useState(0);

  const totalCount = redCount + blueCount + greenCount;

  return (
    <ScrollView
      style={{ flex: 1, backgroundColor: '#1a1a2e' }}
      contentContainerStyle={{ padding: 16, paddingBottom: 80 }}
    >
      <Text
        style={{
          color: '#ffffff',
          fontSize: 20,
          fontWeight: 'bold',
          marginBottom: 20,
        }}
      >
        🎮 计数器测试
      </Text>

      {/* 定位显示组件 */}
      {/* <Geo /> */}

      {/* 播放声音 */}
      <Speech />

      <Weather />

      {/* 总计 */}
      <View
        style={{
          backgroundColor: '#2a2a4e',
          padding: 16,
          borderRadius: 8,
          marginBottom: 20,
          marginTop: 20,
          alignItems: 'center',
        }}
      >
        <Text style={{ color: '#888888', fontSize: 12, marginBottom: 4 }}>
          总点击次数
        </Text>
        <Text style={{ color: '#ffaa00', fontSize: 36, fontWeight: 'bold' }}>
          {totalCount}
        </Text>
      </View>

      {/* 红色按钮 */}
      <TouchableOpacity
        onPress={() => {
          setRedCount((c) => c + 1);
          onChange?.('红色按钮', '🔴', '#ff4444');
        }}
        style={{
          backgroundColor: '#ff4444',
          padding: 20,
          borderRadius: 8,
          marginBottom: 12,
          alignItems: 'center',
        }}
      >
        <Text style={{ color: '#ffffff', fontSize: 18, fontWeight: 'bold' }}>
          🔴 红色 +1
        </Text>
        <Text style={{ color: '#ffcccc', fontSize: 14, marginTop: 4 }}>
          点击: {redCount}
        </Text>
      </TouchableOpacity>

      {/* 蓝色按钮 */}
      <TouchableOpacity
        onPress={() => {
          setBlueCount((c) => c + 1);
          onChange?.('蓝色按钮', '🔵', '#4444ff');
        }}
        style={{
          backgroundColor: '#4444ff',
          padding: 20,
          borderRadius: 8,
          marginBottom: 12,
          alignItems: 'center',
        }}
      >
        <Text style={{ color: '#ffffff', fontSize: 18, fontWeight: 'bold' }}>
          🔵 蓝色 +1
        </Text>
        <Text style={{ color: '#ccccff', fontSize: 14, marginTop: 4 }}>
          点击: {blueCount}
        </Text>
      </TouchableOpacity>

      {/* 绿色按钮 */}
      <TouchableOpacity
        onPress={() => {
          setGreenCount((c) => c + 1);
          onChange?.('绿色按钮', '🟢', '#44ff44');
        }}
        style={{
          backgroundColor: '#44ff44',
          padding: 20,
          borderRadius: 8,
          marginBottom: 12,
          alignItems: 'center',
        }}
      >
        <Text style={{ color: '#000000', fontSize: 18, fontWeight: 'bold' }}>
          🟢 绿色 +1
        </Text>
        <Text style={{ color: '#006600', fontSize: 14, marginTop: 4 }}>
          点击: {greenCount}
        </Text>
      </TouchableOpacity>

      {/* 重置按钮 */}
      <TouchableOpacity
        onPress={() => {
          setRedCount(0);
          setBlueCount(0);
          setGreenCount(0);
        }}
        style={{
          backgroundColor: '#666666',
          padding: 16,
          borderRadius: 8,
          marginTop: 20,
          alignItems: 'center',
          borderWidth: 1,
          borderColor: '#888888',
        }}
      >
        <Text style={{ color: '#ffffff', fontSize: 16, fontWeight: 'bold' }}>
          🔄 重置所有计数
        </Text>
      </TouchableOpacity>
    </ScrollView>
  );
}
