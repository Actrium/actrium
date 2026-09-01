import { useState } from 'react';
import { ScrollView, View, Text, TouchableOpacity } from 'keel/guest';
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
        🎮 Count Test
      </Text>

      {/* Position Display Component */}
      {/* <Geo /> */}

      {/* Play sound */}
      <Speech />

      <Weather />

      {/* Total */}
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
          Total number of hits
        </Text>
        <Text style={{ color: '#ffaa00', fontSize: 36, fontWeight: 'bold' }}>
          {totalCount}
        </Text>
      </View>

      {/* Red button */}
      <TouchableOpacity
        onPress={() => {
          setRedCount((c) => c + 1);
          onChange?.('Red button', '🔴', '#ff4444');
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
          🔴 Red +1
        </Text>
        <Text style={{ color: '#ffcccc', fontSize: 14, marginTop: 4 }}>
          Click: {redCount}
        </Text>
      </TouchableOpacity>

      {/* Blue button */}
      <TouchableOpacity
        onPress={() => {
          setBlueCount((c) => c + 1);
          onChange?.('Blue button', '🔵', '#4444ff');
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
          🔵 Blue +1
        </Text>
        <Text style={{ color: '#ccccff', fontSize: 14, marginTop: 4 }}>
          Click: {blueCount}
        </Text>
      </TouchableOpacity>

      {/* Green button */}
      <TouchableOpacity
        onPress={() => {
          setGreenCount((c) => c + 1);
          onChange?.('Green button', '🟢', '#44ff44');
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
          🟢 Green +1
        </Text>
        <Text style={{ color: '#006600', fontSize: 14, marginTop: 4 }}>
          Click: {greenCount}
        </Text>
      </TouchableOpacity>

      {/* Reset button */}
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
          🔄 Reset All Counts
        </Text>
      </TouchableOpacity>
    </ScrollView>
  );
}
