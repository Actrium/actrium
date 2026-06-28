/**
 * Panels - Hook 导出模式
 * Guest 导出 usePanels Hook，Host 直接调用获取 left/right
 * 与 counterapp 保持一致
 */
import React, {useState, useCallback, useMemo, useEffect, useRef} from 'react';
import {View, Text, TouchableOpacity, ScrollView, ActivityIndicator, useConfig, useSendToHost, useHostEvent} from 'rill/guest';

function useReactiveConfig<T>() {
  const [, setTick] = useState(0);
  useHostEvent('CONFIG_UPDATE', () => setTick(t => t + 1));
  return useConfig<T>();
}

// ============ Weather Types ============
interface WeatherData {
  city: string;
  temperature: number;
  description: string;
  humidity: number;
  windSpeed: number;
  feelsLike: number;
  icon: string;
  updatedAt: string;
}

interface FetchResponse {
  requestId: string;
  ok: boolean;
  status: number;
  data?: unknown;
  error?: string;
}

function Tap({
  onPress,
  children,
  style,
  hitSlop,
  disabled,
}: {
  onPress?: () => void;
  children: React.ReactNode;
  style?: any;
  hitSlop?: any;
  disabled?: boolean;
}) {
  const [pressed, setPressed] = useState(false);
  const [flash, setFlash] = useState(false);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
    };
  }, []);

  return (
    <TouchableOpacity
      hitSlop={hitSlop}
      disabled={disabled}
      activeOpacity={0.6}
      delayPressIn={0}
      delayPressOut={0}
      onPress={() => {
        if (disabled) return;
        setPressed(true);
        setFlash(true);
        if (timeoutRef.current) clearTimeout(timeoutRef.current);
        timeoutRef.current = setTimeout(() => {
          setPressed(false);
          setFlash(false);
        }, 160);
        onPress?.();
      }}
      onPressIn={() => {
        if (disabled) return;
        setPressed(true);
      }}
      onPressOut={() => {
        if (disabled) return;
        if (timeoutRef.current) clearTimeout(timeoutRef.current);
        timeoutRef.current = setTimeout(() => setPressed(false), 90);
      }}
    >
      <View style={[style, { position: 'relative' }, pressed ? { transform: [{ scale: 0.98 }], opacity: 0.85 } : null, disabled ? { opacity: 0.5 } : null]}>
        {children}
        {(pressed || flash) && (
          <View
            pointerEvents="none"
            style={{
              position: 'absolute',
              left: 0,
              top: 0,
              right: 0,
              bottom: 0,
              borderRadius: style?.borderRadius ?? 8,
              backgroundColor: 'rgba(255,255,255,0.22)',
            }}
          />
        )}
      </View>
    </TouchableOpacity>
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

  // Clock state (with seconds)
  const [currentTime, setCurrentTime] = useState(() => {
    const now = new Date();
    return `${now.getHours().toString().padStart(2, '0')}:${now.getMinutes().toString().padStart(2, '0')}:${now.getSeconds().toString().padStart(2, '0')}`;
  });

  // Weather state
  const [weatherData, setWeatherData] = useState<WeatherData | null>(null);
  const [weatherLoading, setWeatherLoading] = useState(false);
  const [weatherExpanded, setWeatherExpanded] = useState(false);

  // Host communication
  const sendToHost = useSendToHost();
  const pendingRequestRef = useRef<string | null>(null);

  // Clock update effect
  useEffect(() => {
    const updateTime = () => {
      const now = new Date();
      setCurrentTime(`${now.getHours().toString().padStart(2, '0')}:${now.getMinutes().toString().padStart(2, '0')}:${now.getSeconds().toString().padStart(2, '0')}`);
    };
    const interval = setInterval(updateTime, 1000);
    return () => clearInterval(interval);
  }, []);

  // Listen for fetch responses from Host
  useHostEvent<FetchResponse>('APP_FETCH_RESPONSE', (response) => {
    if (response.requestId !== pendingRequestRef.current) return;
    pendingRequestRef.current = null;
    setWeatherLoading(false);

    if (!response.ok || !response.data) return;

    // Parse Open-Meteo response
    const data = response.data as {
      current_weather?: {
        temperature?: number;
        windspeed?: number;
        weathercode?: number;
      };
      hourly?: {
        relativehumidity_2m?: number[];
      };
    };

    if (data.current_weather) {
      const now = new Date();
      const temp = data.current_weather.temperature ?? 0;
      const windSpeed = data.current_weather.windspeed ?? 0;
      const humidity = data.hourly?.relativehumidity_2m?.[0] ?? 0;
      const weatherCode = data.current_weather.weathercode ?? 0;

      // Map weather code to description
      const weatherDescriptions: Record<number, string> = {
        0: 'Clear',
        1: 'Mainly Clear',
        2: 'Partly Cloudy',
        3: 'Overcast',
        45: 'Fog',
        48: 'Fog',
        51: 'Light Drizzle',
        53: 'Drizzle',
        55: 'Heavy Drizzle',
        61: 'Light Rain',
        63: 'Rain',
        65: 'Heavy Rain',
        71: 'Light Snow',
        73: 'Snow',
        75: 'Heavy Snow',
        95: 'Thunderstorm',
      };

      setWeatherData({
        city: 'Shanghai',
        temperature: Math.round(temp),
        description: weatherDescriptions[weatherCode] || 'Unknown',
        humidity: humidity,
        windSpeed: Math.round(windSpeed),
        feelsLike: Math.round(temp), // Open-Meteo doesn't provide feels_like in basic API
        icon: weatherCode <= 3 ? '01d' : '03d',
        updatedAt: `${now.getHours().toString().padStart(2, '0')}:${now.getMinutes().toString().padStart(2, '0')}`,
      });
      setWeatherExpanded(true);
    }
  });

  // Fetch weather data
  const fetchWeather = useCallback(() => {
    if (weatherLoading) return;

    setWeatherLoading(true);
    const requestId = `weather-${Date.now()}`;
    pendingRequestRef.current = requestId;

    // Open-Meteo API - Shanghai (free, no API key needed)
    const url = 'https://api.open-meteo.com/v1/forecast?latitude=31.23&longitude=121.47&current_weather=true&hourly=relativehumidity_2m';

    sendToHost('APP_FETCH_REQUEST', { requestId, url, options: { method: 'GET' } });

    // Timeout fallback
    setTimeout(() => {
      if (pendingRequestRef.current === requestId) {
        pendingRequestRef.current = null;
        setWeatherLoading(false);
      }
    }, 10000);
  }, [weatherLoading, sendToHost]);

  const totalCount = redCount + blueCount + greenCount;

  return (
    <ScrollView
      style={{flex: 1, backgroundColor: '#1a1a2e'}}
      contentContainerStyle={{padding: 16, paddingBottom: 80}}>
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
      <Tap
        onPress={() => {
          console.log('[CLICK_TEST] Red button pressed, count=' + (redCount + 1));
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
      </Tap>

      {/* 蓝色按钮 */}
      <Tap
        onPress={() => {
          console.log('[CLICK_TEST] Blue button pressed, count=' + (blueCount + 1));
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
      </Tap>

      {/* 绿色按钮 */}
      <Tap
        onPress={() => {
          console.log('[CLICK_TEST] Green button pressed, count=' + (greenCount + 1));
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
      </Tap>

      {/* 重置按钮 */}
      <Tap
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
      </Tap>

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

      {/* Bottom Toolbar: Clock + Weather */}
      <View
        style={{
          marginTop: 20,
          padding: 12,
          backgroundColor: '#2a2a4e',
          borderRadius: 8,
          flexDirection: 'row',
          alignItems: 'center',
          justifyContent: 'space-between',
        }}>
        {/* Clock */}
        <Text style={{color: '#888888', fontSize: 14, fontVariant: ['tabular-nums']}}>
          {currentTime}
        </Text>

        {/* Weather Button */}
        <Tap
          onPress={fetchWeather}
          disabled={weatherLoading}
          style={{
            padding: 8,
            backgroundColor: weatherExpanded ? '#1a365d' : '#3a3a5e',
            borderRadius: 6,
            flexDirection: 'row',
            alignItems: 'center',
          }}>
          {weatherLoading ? (
            <ActivityIndicator size="small" color="#88b8e8" />
          ) : (
            <Text style={{color: '#88b8e8', fontSize: 14}}>
              {weatherData ? `${weatherData.temperature}°C` : '🌤️'}
            </Text>
          )}
        </Tap>
      </View>

      {/* Expandable Weather Card */}
      {weatherExpanded && weatherData && (
        <View
          style={{
            marginTop: 12,
            padding: 16,
            backgroundColor: '#1a365d',
            borderRadius: 12,
            borderWidth: 1,
            borderColor: '#2d4a77',
          }}>
          {/* Header */}
          <View
            style={{
              flexDirection: 'row',
              justifyContent: 'space-between',
              alignItems: 'center',
              marginBottom: 12,
            }}>
            <Text style={{color: '#ffffff', fontSize: 16, fontWeight: 'bold'}}>
              {weatherData.city}
            </Text>
            <Tap hitSlop={{ top: 8, left: 8, right: 8, bottom: 8 }} onPress={() => setWeatherExpanded(false)}>
              <Text style={{color: '#88b8e8', fontSize: 12}}>
                收起 ▲
              </Text>
            </Tap>
          </View>

          {/* Main Info */}
          <View
            style={{
              flexDirection: 'row',
              alignItems: 'baseline',
              marginBottom: 12,
            }}>
            <Text style={{color: '#ffffff', fontSize: 36, fontWeight: '300', marginRight: 12}}>
              {weatherData.temperature}°C
            </Text>
            <Text style={{color: '#a0c4e8', fontSize: 16}}>
              {weatherData.description}
            </Text>
          </View>

          {/* Details */}
          <View style={{flexDirection: 'row', flexWrap: 'wrap', marginBottom: 8}}>
            <Text style={{color: '#c0d8f0', fontSize: 12, marginRight: 16}}>
              体感 {weatherData.feelsLike}°C
            </Text>
            <Text style={{color: '#c0d8f0', fontSize: 12, marginRight: 16}}>
              湿度 {weatherData.humidity}%
            </Text>
            <Text style={{color: '#c0d8f0', fontSize: 12}}>
              风速 {weatherData.windSpeed}km/h
            </Text>
          </View>

          {/* Update Time */}
          <Text style={{color: '#88b8e8', fontSize: 11, fontStyle: 'italic'}}>
            更新于 {weatherData.updatedAt}
          </Text>
        </View>
      )}

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

        <Tap
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
        </Tap>

        <Tap
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
        </Tap>

        <Tap
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
        </Tap>
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
            <Tap hitSlop={{ top: 8, left: 8, right: 8, bottom: 8 }} onPress={onClear} style={{padding: 4}}>
              <Text style={{color: '#ff6666', fontSize: 11}}>清空</Text>
            </Tap>
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
  const config = useReactiveConfig<{ containerWidth?: number; containerHeight?: number; isOverviewMode?: boolean }>();
  const isCompact =
    config.isOverviewMode === true ||
    (typeof config.containerWidth === 'number' && config.containerWidth > 0 && config.containerWidth < 520) ||
    (typeof config.containerHeight === 'number' && config.containerHeight > 0 && config.containerHeight < 360);

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

  const left = useMemo(() => {
    if (isCompact) {
      return (
        <View
          style={{
            flex: 1,
            justifyContent: 'center',
            alignItems: 'center',
            padding: 12,
            backgroundColor: '#101022',
          }}
        >
          <Text style={{ color: '#ffffff', fontSize: 18, fontWeight: '700' }}>
            Count: {sessionClicks}
          </Text>
        </View>
      );
    }
    return <Counter onChange={addRecord} />;
  }, [isCompact, addRecord, sessionClicks]);

  const right = useMemo(() => {
    if (isCompact) {
      return <View style={{ flex: 1, backgroundColor: '#0b0b12' }} />;
    }
    return (
      <Stats
        records={records}
        sessionClicks={sessionClicks}
        onAddRecord={addRecord}
        onClear={clearRecords}
      />
    );
  }, [isCompact, records, sessionClicks, addRecord, clearRecords]);

  return { left, right };
}
