/**
 * Panels - Hook Export Mode
 * Guest Export usePanels Hook, Host Direct Call Access left/right
 * with counterapp Consistency
 */
import { useState, useCallback, useRef } from 'react';
import { View, Text, TouchableOpacity, ActivityIndicator } from 'keel/guest';
import dayjs from 'dayjs';
import { useHttp } from 'askit';

import { WeatherData } from '../types';

type TWeather = {
  current_weather?: {
    temperature?: number;
    windspeed?: number;
    weathercode?: number;
  };
  hourly?: {
    relativehumidity_2m?: number[];
  };
};

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

export default function Weather({}: {
  onChange?: (action: string, emoji: string, color: string) => void;
}) {
  // Weather state
  const [weatherData, setWeatherData] = useState<WeatherData | null>(null);
  const [weatherLoading, setWeatherLoading] = useState(false);
  const [weatherExpanded, setWeatherExpanded] = useState(false);

  // Host communication
  const { get } = useHttp();
  const pendingRequestRef = useRef<string | null>(null);

  const fetchWeather = useCallback(() => {
    if (weatherLoading) return;

    const url =
      'https://api.open-meteo.com/v1/forecast?latitude=31.23&longitude=121.47&current_weather=true&hourly=relativehumidity_2m';
    const requestId = `weather-${Date.now()}`;

    setWeatherLoading(true);
    pendingRequestRef.current = requestId;

    get<TWeather>(requestId, url)
      .then((response) => {
        console.log(response);
        if (response.requestId !== pendingRequestRef.current) return;
        pendingRequestRef.current = null;
        setWeatherLoading(false);

        if (!response.success || !response.data) return;

        const data = response.data;
        if (data.current_weather) {
          const now = new Date();
          const temp = data.current_weather.temperature ?? 0;
          const windSpeed = data.current_weather.windspeed ?? 0;
          const humidity = data.hourly?.relativehumidity_2m?.[0] ?? 0;
          const weatherCode = data.current_weather.weathercode ?? 0;

          setWeatherData({
            city: 'Shanghai',
            temperature: Math.round(temp),
            description: weatherDescriptions[weatherCode] || 'Unknown',
            humidity: humidity,
            windSpeed: Math.round(windSpeed),
            feelsLike: Math.round(temp),
            icon: weatherCode <= 3 ? '01d' : '03d',
            updatedAt: dayjs(now).format('HH:mm'),
          });
          setWeatherExpanded(true);
        }
      })
      .catch((err) => {
        console.error('[Weather] Fetch failed:', err);
        setWeatherLoading(false);
        pendingRequestRef.current = null;
      });

    // Timeout fallback
    setTimeout(() => {
      if (pendingRequestRef.current === requestId) {
        pendingRequestRef.current = null;
        setWeatherLoading(false);
      }
    }, 10000);
  }, [weatherLoading, get]);

  return (
    <View>
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
        }}
      >
        {/* Clock */}
        <Text
          style={{
            color: '#888888',
            fontSize: 14,
            fontVariant: ['tabular-nums'],
          }}
        >
          {dayjs().format('HH:mm:ss')}
        </Text>

        {/* Weather Button */}
        <TouchableOpacity
          onPress={fetchWeather}
          disabled={weatherLoading}
          style={{
            padding: 8,
            backgroundColor: weatherExpanded ? '#1a365d' : '#3a3a5e',
            borderRadius: 6,
            flexDirection: 'row',
            alignItems: 'center',
          }}
        >
          {weatherLoading ? (
            <ActivityIndicator size='small' color='#88b8e8' />
          ) : (
            <Text style={{ color: '#88b8e8', fontSize: 14 }}>
              {weatherData ? `${weatherData.temperature}°C` : '🌤️'}
            </Text>
          )}
        </TouchableOpacity>
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
          }}
        >
          {/* Header */}
          <View
            style={{
              flexDirection: 'row',
              justifyContent: 'space-between',
              alignItems: 'center',
              marginBottom: 12,
            }}
          >
            <Text
              style={{ color: '#ffffff', fontSize: 16, fontWeight: 'bold' }}
            >
              {weatherData.city}
            </Text>
            <TouchableOpacity onPress={() => setWeatherExpanded(false)}>
              <Text style={{ color: '#88b8e8', fontSize: 12 }}>Put it away. ▲</Text>
            </TouchableOpacity>
          </View>

          {/* Main Info */}
          <View
            style={{
              flexDirection: 'row',
              alignItems: 'baseline',
              marginBottom: 12,
            }}
          >
            <Text
              style={{
                color: '#ffffff',
                fontSize: 36,
                fontWeight: '300',
                marginRight: 12,
              }}
            >
              {weatherData.temperature}°C
            </Text>
            <Text style={{ color: '#a0c4e8', fontSize: 16 }}>
              {weatherData.description}
            </Text>
          </View>

          {/* Details */}
          <View
            style={{ flexDirection: 'row', flexWrap: 'wrap', marginBottom: 8 }}
          >
            <Text style={{ color: '#c0d8f0', fontSize: 12, marginRight: 16 }}>
              Sensory. {weatherData.feelsLike}°C
            </Text>
            <Text style={{ color: '#c0d8f0', fontSize: 12, marginRight: 16 }}>
              Humidity {weatherData.humidity}%
            </Text>
            <Text style={{ color: '#c0d8f0', fontSize: 12 }}>
              Wind speed {weatherData.windSpeed}km/h
            </Text>
          </View>

          {/* Update Time */}
          <Text style={{ color: '#88b8e8', fontSize: 11, fontStyle: 'italic' }}>
            Update on {weatherData.updatedAt}
          </Text>
        </View>
      )}
    </View>
  );
}
