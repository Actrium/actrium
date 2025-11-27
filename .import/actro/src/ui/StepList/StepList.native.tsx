/**
 * StepList - Native Implementation (Host App)
 *
 * A step-by-step progress indicator component with connecting lines.
 * Supports loop display mode and various status states.
 */

import React, { useMemo } from 'react';
import {
  View,
  Text,
  StyleSheet,
  TouchableOpacity,
  type ViewStyle,
  type TextStyle,
} from 'react-native';
import type { StepListProps, StepItem, StepStatus } from '../../types';

// Default colors
const DEFAULT_COLORS = {
  active: '#007AFF',
  completed: '#34C759',
  pending: '#C7C7CC',
  error: '#FF3B30',
};

interface StepNodeProps {
  item: StepItem;
  index: number;
  isLast: boolean;
  loop: boolean;
  colors: typeof DEFAULT_COLORS;
  lineWidth: number;
  onPress?: (item: StepItem, index: number) => void;
  key?: React.Key;
}

/**
 * Individual step node with icon, title, subtitle and connecting line
 */
function StepNode({ item, index, isLast, loop, colors, lineWidth, onPress }: StepNodeProps) {
  const statusColor = getStatusColor(item.status, colors);
  const showLine = !isLast || loop;

  const handlePress = () => {
    onPress?.(item, index);
  };

  return (
    <TouchableOpacity
      style={styles.stepContainer}
      onPress={handlePress}
      disabled={!onPress}
      activeOpacity={onPress ? 0.7 : 1}
    >
      {/* Left side: Icon and Line */}
      <View style={styles.leftColumn}>
        {/* Step Circle/Icon */}
        <View
          style={[styles.stepCircle, { backgroundColor: statusColor, borderColor: statusColor }]}
        >
          {item.status === 'completed' ? (
            <Text style={styles.checkmark}>✓</Text>
          ) : item.status === 'error' ? (
            <Text style={styles.errorMark}>!</Text>
          ) : (
            <Text style={styles.stepNumber}>{index + 1}</Text>
          )}
        </View>

        {/* Connecting Line */}
        {showLine && (
          <View
            style={[
              styles.line,
              {
                backgroundColor: isLast ? colors.pending : statusColor,
                width: lineWidth,
              },
            ]}
          />
        )}
      </View>

      {/* Right side: Content */}
      <View style={styles.contentColumn}>
        <Text style={[styles.title, { color: item.status === 'pending' ? '#8E8E93' : '#000000' }]}>
          {item.title}
        </Text>
        {item.subtitle && <Text style={styles.subtitle}>{item.subtitle}</Text>}
      </View>
    </TouchableOpacity>
  );
}

/**
 * Get color for status
 */
function getStatusColor(status: StepStatus, colors: typeof DEFAULT_COLORS): string {
  switch (status) {
    case 'active':
      return colors.active;
    case 'completed':
      return colors.completed;
    case 'error':
      return colors.error;
    case 'pending':
    default:
      return colors.pending;
  }
}

/**
 * StepList Component
 */
export function StepList({
  items,
  loop = false,
  onStepPress,
  activeColor = DEFAULT_COLORS.active,
  completedColor = DEFAULT_COLORS.completed,
  pendingColor = DEFAULT_COLORS.pending,
  errorColor = DEFAULT_COLORS.error,
  lineWidth = 2,
  style,
}: StepListProps) {
  const colors = useMemo(
    () => ({
      active: activeColor,
      completed: completedColor,
      pending: pendingColor,
      error: errorColor,
    }),
    [activeColor, completedColor, pendingColor, errorColor]
  );

  return (
    <View style={[styles.container, style as ViewStyle]}>
      {items.map((item, index) => (
        <StepNode
          key={item.id}
          item={item}
          index={index}
          isLast={index === items.length - 1}
          loop={loop}
          colors={colors}
          lineWidth={lineWidth}
          onPress={onStepPress}
        />
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flexDirection: 'column',
  },
  stepContainer: {
    flexDirection: 'row',
    minHeight: 60,
  },
  leftColumn: {
    alignItems: 'center',
    width: 40,
  },
  stepCircle: {
    width: 28,
    height: 28,
    borderRadius: 14,
    borderWidth: 2,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: '#FFFFFF',
  },
  stepNumber: {
    fontSize: 12,
    fontWeight: '600',
    color: '#FFFFFF',
  } as TextStyle,
  checkmark: {
    fontSize: 14,
    fontWeight: 'bold',
    color: '#FFFFFF',
  } as TextStyle,
  errorMark: {
    fontSize: 14,
    fontWeight: 'bold',
    color: '#FFFFFF',
  } as TextStyle,
  line: {
    flex: 1,
    width: 2,
    marginTop: 4,
    marginBottom: 4,
  },
  contentColumn: {
    flex: 1,
    paddingLeft: 12,
    paddingBottom: 16,
    justifyContent: 'center',
  },
  title: {
    fontSize: 16,
    fontWeight: '500',
    color: '#000000',
  } as TextStyle,
  subtitle: {
    fontSize: 14,
    color: '#8E8E93',
    marginTop: 4,
  } as TextStyle,
});

export default StepList;
