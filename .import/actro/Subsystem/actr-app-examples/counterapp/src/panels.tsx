/**
 * Panels - Hook 导出模式
 * Guest 导出 usePanels Hook，Host 直接调用获取 left/right
 * 与 counterapp 保持一致
 */
import React, {useState, useEffect, useCallback, useMemo} from 'react';
import {View, Text, TouchableOpacity, ScrollView} from 'rill/let';

// ============ Clock Component ============
function Clock() {
  const [time, setTime] = useState(new Date());

  useEffect(() => {
    const timer = setInterval(() => {
      setTime(new Date());
    }, 1000);

    return () => clearInterval(timer);
  }, []);

  const hours = time.getHours().toString().padStart(2, '0');
  const minutes = time.getMinutes().toString().padStart(2, '0');
  const seconds = time.getSeconds().toString().padStart(2, '0');

  return (
    <View
      style={{
        marginTop: 20,
        padding: 20,
        backgroundColor: '#0a0a1a',
        borderRadius: 12,
        alignItems: 'center',
        borderWidth: 2,
        borderColor: '#333366',
      }}>
      <Text style={{color: '#666688', fontSize: 12, marginBottom: 8}}>
        🕐 实时时钟
      </Text>
      <Text
        style={{
          color: '#00ff88',
          fontSize: 48,
          fontWeight: 'bold',
          fontFamily: 'Menlo',
          letterSpacing: 4,
        }}>
        {hours}:{minutes}:{seconds}
      </Text>
    </View>
  );
}

// ============ Left Panel: Counter ============
function Counter({
  onChange,
}: {
  onChange?: (action: string, emoji: string, color: string) => void;
}) {
  const [redCount, setRedCount] = useState(0);
  const [blueCount, setBlueCount] = useState(0);
  const [greenCount, setGreenCount] = useState(0);

  const totalCount = redCount + blueCount + greenCount;

  return (
    <ScrollView
      style={{flex: 1, backgroundColor: '#1a1a2e'}}
      contentContainerStyle={{padding: 16}}>
      <Text
        style={{
          color: '#ffffff',
          fontSize: 20,
          fontWeight: 'bold',
          marginBottom: 20,
        }}>
        🎮 计数器测试
      </Text>

      {/* 总计 */}
      <View
        style={{
          backgroundColor: '#2a2a4e',
          padding: 16,
          borderRadius: 8,
          marginBottom: 20,
          alignItems: 'center',
        }}>
        <Text style={{color: '#888888', fontSize: 12, marginBottom: 4}}>
          总点击次数
        </Text>
        <Text style={{color: '#ffaa00', fontSize: 36, fontWeight: 'bold'}}>
          {totalCount}
        </Text>
      </View>

      {/* 红色按钮 */}
      <TouchableOpacity
        onPress={() => {
          setRedCount(c => c + 1);
          onChange?.('红色按钮', '🔴', '#ff4444');
        }}
        style={{
          backgroundColor: '#ff4444',
          padding: 20,
          borderRadius: 8,
          marginBottom: 12,
          alignItems: 'center',
        }}>
        <Text style={{color: '#ffffff', fontSize: 18, fontWeight: 'bold'}}>
          🔴 红色 +1
        </Text>
        <Text style={{color: '#ffcccc', fontSize: 14, marginTop: 4}}>
          点击: {redCount}
        </Text>
      </TouchableOpacity>

      {/* 蓝色按钮 */}
      <TouchableOpacity
        onPress={() => {
          setBlueCount(c => c + 1);
          onChange?.('蓝色按钮', '🔵', '#4444ff');
        }}
        style={{
          backgroundColor: '#4444ff',
          padding: 20,
          borderRadius: 8,
          marginBottom: 12,
          alignItems: 'center',
        }}>
        <Text style={{color: '#ffffff', fontSize: 18, fontWeight: 'bold'}}>
          🔵 蓝色 +1
        </Text>
        <Text style={{color: '#ccccff', fontSize: 14, marginTop: 4}}>
          点击: {blueCount}
        </Text>
      </TouchableOpacity>

      {/* 绿色按钮 */}
      <TouchableOpacity
        onPress={() => {
          setGreenCount(c => c + 1);
          onChange?.('绿色按钮', '🟢', '#44ff44');
        }}
        style={{
          backgroundColor: '#44ff44',
          padding: 20,
          borderRadius: 8,
          marginBottom: 12,
          alignItems: 'center',
        }}>
        <Text style={{color: '#000000', fontSize: 18, fontWeight: 'bold'}}>
          🟢 绿色 +1
        </Text>
        <Text style={{color: '#006600', fontSize: 14, marginTop: 4}}>
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
        }}>
        <Text style={{color: '#ffffff', fontSize: 16, fontWeight: 'bold'}}>
          🔄 重置所有计数
        </Text>
      </TouchableOpacity>

      {/* 说明 */}
      <View
        style={{
          marginTop: 30,
          padding: 12,
          backgroundColor: '#2a2a3a',
          borderRadius: 8,
          borderLeftWidth: 4,
          borderLeftColor: '#ffaa00',
        }}>
        <Text style={{color: '#cccccc', fontSize: 11, lineHeight: 16}}>
          💡 点击彩色按钮增加对应计数{'\n'}
          📊 总计数显示在顶部{'\n'}
          🔄 重置按钮清空所有计数{'\n'}
          ⚡ 左侧点击会同步到右侧面板
        </Text>
      </View>

      {/* 实时时钟 */}
      <Clock />
    </ScrollView>
  );
}

// ============ Right Panel: Stats ============
interface ClickRecord {
  id: number;
  time: string;
  action: string;
  emoji: string;
  color: string;
}

function Stats({
  records,
  sessionClicks,
  onAddRecord,
  onClear,
}: {
  records: ClickRecord[];
  sessionClicks: number;
  onAddRecord: (action: string, emoji: string, color: string) => void;
  onClear: () => void;
}) {
  return (
    <ScrollView
      style={{flex: 1, backgroundColor: '#1a1a2e'}}
      contentContainerStyle={{padding: 16}}>
      <Text
        style={{
          color: '#ffffff',
          fontSize: 20,
          fontWeight: 'bold',
          marginBottom: 20,
        }}>
        📊 交互统计
      </Text>

      {/* 会话统计 */}
      <View
        style={{
          backgroundColor: '#2a2a4e',
          padding: 16,
          borderRadius: 8,
          marginBottom: 20,
        }}>
        <Text style={{color: '#888888', fontSize: 12, marginBottom: 8}}>
          本次会话点击
        </Text>
        <Text style={{color: '#00ff88', fontSize: 32, fontWeight: 'bold'}}>
          {sessionClicks}
        </Text>
        <Text style={{color: '#666666', fontSize: 10, marginTop: 4}}>
          历史记录: {records.length} 条
        </Text>
      </View>

      {/* 快速测试按钮 */}
      <View style={{marginBottom: 20}}>
        <Text style={{color: '#888888', fontSize: 12, marginBottom: 8}}>
          快速测试
        </Text>

        <TouchableOpacity
          onPress={() => onAddRecord('快速点击', '⚡', '#ffaa00')}
          style={{
            backgroundColor: '#ffaa00',
            padding: 12,
            borderRadius: 6,
            marginBottom: 8,
            alignItems: 'center',
          }}>
          <Text style={{color: '#000000', fontSize: 14, fontWeight: 'bold'}}>
            ⚡ 快速测试
          </Text>
        </TouchableOpacity>

        <TouchableOpacity
          onPress={() => onAddRecord('星标动作', '⭐', '#ff88ff')}
          style={{
            backgroundColor: '#ff88ff',
            padding: 12,
            borderRadius: 6,
            marginBottom: 8,
            alignItems: 'center',
          }}>
          <Text style={{color: '#000000', fontSize: 14, fontWeight: 'bold'}}>
            ⭐ 星标测试
          </Text>
        </TouchableOpacity>

        <TouchableOpacity
          onPress={() => onAddRecord('火箭发射', '🚀', '#00aaff')}
          style={{
            backgroundColor: '#00aaff',
            padding: 12,
            borderRadius: 6,
            alignItems: 'center',
          }}>
          <Text style={{color: '#ffffff', fontSize: 14, fontWeight: 'bold'}}>
            🚀 火箭测试
          </Text>
        </TouchableOpacity>
      </View>

      {/* 点击历史 */}
      <View>
        <View
          style={{
            flexDirection: 'row',
            justifyContent: 'space-between',
            alignItems: 'center',
            marginBottom: 12,
          }}>
          <Text style={{color: '#888888', fontSize: 12}}>点击历史</Text>
          {records.length > 0 && (
            <TouchableOpacity onPress={onClear} style={{padding: 4}}>
              <Text style={{color: '#ff6666', fontSize: 11}}>清空</Text>
            </TouchableOpacity>
          )}
        </View>

        {records.length === 0 ? (
          <View
            style={{
              backgroundColor: '#2a2a3a',
              padding: 20,
              borderRadius: 8,
              alignItems: 'center',
            }}>
            <Text style={{color: '#666666', fontSize: 13}}>暂无记录</Text>
            <Text style={{color: '#555555', fontSize: 11, marginTop: 4}}>
              点击上方按钮开始测试
            </Text>
          </View>
        ) : (
          records.map(record => (
            <View
              key={record.id}
              style={{
                backgroundColor: '#2a2a3a',
                padding: 12,
                borderRadius: 6,
                marginBottom: 8,
                borderLeftWidth: 3,
                borderLeftColor: record.color,
              }}>
              <View
                style={{
                  flexDirection: 'row',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                }}>
                <View style={{flexDirection: 'row', alignItems: 'center'}}>
                  <Text style={{fontSize: 16, marginRight: 8}}>
                    {record.emoji}
                  </Text>
                  <Text style={{color: '#ffffff', fontSize: 13}}>
                    {record.action}
                  </Text>
                </View>
                <Text style={{color: '#666666', fontSize: 11}}>
                  {record.time}
                </Text>
              </View>
            </View>
          ))
        )}
      </View>

      {/* 说明 */}
      <View
        style={{
          marginTop: 20,
          padding: 12,
          backgroundColor: '#2a2a3a',
          borderRadius: 8,
          borderLeftWidth: 4,
          borderLeftColor: '#00ff88',
        }}>
        <Text style={{color: '#cccccc', fontSize: 11, lineHeight: 16}}>
          📊 实时统计点击次数{'\n'}
          ⚡ 快速测试按钮验证交互{'\n'}
          📜 自动记录最近20次操作
        </Text>
      </View>
    </ScrollView>
  );
}

// ============ 导出 Hook ============
export function usePanels() {
  const [records, setRecords] = useState<ClickRecord[]>([]);
  const [sessionClicks, setSessionClicks] = useState(0);

  const addRecord = useCallback((action: string, emoji: string, color: string) => {
    const now = new Date();
    const timeStr = `${now.getHours().toString().padStart(2, '0')}:${now
      .getMinutes()
      .toString()
      .padStart(2, '0')}:${now.getSeconds().toString().padStart(2, '0')}`;

    setRecords(prev =>
      [{id: Date.now(), time: timeStr, action, emoji, color}, ...prev].slice(
        0,
        20,
      ),
    );
    setSessionClicks(c => c + 1);
  }, []);

  const clearRecords = useCallback(() => {
    setRecords([]);
    setSessionClicks(0);
  }, []);

  const left = useMemo(() => <Counter onChange={addRecord} />, [addRecord]);
  const right = useMemo(() => (
    <Stats
      records={records}
      sessionClicks={sessionClicks}
      onAddRecord={addRecord}
      onClear={clearRecords}
    />
  ), [records, sessionClicks, addRecord, clearRecords]);

  return { left, right };
}
