import React from 'react';
import { StyleSheet, TouchableOpacity, View } from 'react-native';
import Animated, {
  Extrapolation,
  interpolate,
  SharedValue,
  useAnimatedStyle,
} from 'react-native-reanimated';
import ReanimatedSwipeable, {
  SwipeableMethods,
} from 'react-native-gesture-handler/ReanimatedSwipeable';
import { Icon } from '../../components';

type SwipeRemoveActionProps = {
  progress: SharedValue<number>;
  onPress: () => void;
};

function SwipeRemoveAction({ progress, onPress }: SwipeRemoveActionProps) {
  const actionAnimatedStyle = useAnimatedStyle(() => {
    const opacity = interpolate(
      progress.value,
      [0, 0.08, 0.25, 1],
      [0, 0, 0.4, 1],
      Extrapolation.CLAMP,
    );
    const translateX = interpolate(
      progress.value,
      [0, 1],
      [8, 0],
      Extrapolation.CLAMP,
    );
    return {
      opacity,
      transform: [{ translateX }],
    };
  }, [progress]);

  return (
    <View style={styles.deleteUnderlay}>
      <Animated.View style={actionAnimatedStyle}>
        <TouchableOpacity
          style={styles.deleteAction}
          onPress={onPress}
          activeOpacity={0.85}
        >
          <Icon type='remove_star' size={17} disableTint />
        </TouchableOpacity>
      </Animated.View>
    </View>
  );
}

type SwipeRemoveActionRowProps = {
  itemId: number;
  children: React.ReactNode;
  onRemovePress: (methods: SwipeableMethods) => void;
  swipeableRefs: React.MutableRefObject<Record<number, SwipeableMethods | null>>;
  openedSwipeItemIdRef: React.MutableRefObject<number | null>;
  setOpenedSwipeItemId: React.Dispatch<React.SetStateAction<number | null>>;
};

export function SwipeRemoveActionRow({
  itemId,
  children,
  onRemovePress,
  swipeableRefs,
  openedSwipeItemIdRef,
  setOpenedSwipeItemId,
}: SwipeRemoveActionRowProps) {
  return (
    <ReanimatedSwipeable
      rightThreshold={40}
      overshootRight={false}
      friction={1.4}
      overshootFriction={8}
      renderRightActions={(progress, _translation, methods) => {
        swipeableRefs.current[itemId] = methods;
        return (
          <SwipeRemoveAction
            progress={progress}
            onPress={() => onRemovePress(methods)}
          />
        );
      }}
      onSwipeableOpenStartDrag={() => {
        const openedId = openedSwipeItemIdRef.current;
        if (openedId !== null && openedId !== itemId) {
          swipeableRefs.current[openedId]?.close();
        }
      }}
      onSwipeableWillOpen={() => {
        const openedId = openedSwipeItemIdRef.current;
        if (openedId !== null && openedId !== itemId) {
          swipeableRefs.current[openedId]?.close();
        }
        openedSwipeItemIdRef.current = itemId;
        setOpenedSwipeItemId(itemId);
      }}
      onSwipeableClose={() => {
        if (openedSwipeItemIdRef.current === itemId) {
          openedSwipeItemIdRef.current = null;
          setOpenedSwipeItemId(null);
        }
      }}
    >
      {children}
    </ReanimatedSwipeable>
  );
}

const styles = StyleSheet.create({
  deleteUnderlay: {
    width: 72,
    height: 52,
    justifyContent: 'center',
    alignItems: 'flex-end',
    paddingRight: 16,
  },
  deleteAction: {
    width: 56,
    height: 52,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: '#FF453A',
  },
});
