import React, { useEffect, useRef } from 'react';
import { NativeModules, Pressable, Text, View, useColorScheme, type ViewStyle } from 'react-native';
import type { MenuOption, MenuProps, SelectProps } from 'keel/guest';
import { Svg } from './guest-svg-renderer';
import { guestMenuEntries, isGuestMenuOption } from './guest-menu-options';
import { completeGuestMenu } from './guest-menu-selection';

export const guestMenuNames = ['Select', 'Menu'] as const;
type Component = typeof guestMenuNames[number];
type Anchor = { x: number; y: number; width: number; height: number };
type MenuContent = { component: Component; anchor: Anchor; items: readonly MenuOption[]; selectedValue?: string };
type NativeGuestMenu = { show(tabId: string, reloadKey: number, contentJSON: string): Promise<string | null> };
type ShowMenu = (content: MenuContent) => Promise<string | null>;

type TriggerProps = {
  component: Component;
  title: string;
  items: readonly MenuOption[];
  selectedValue?: string;
  disabled?: boolean;
  accessibilityLabel?: string;
  testID?: string;
  style?: SelectProps['style'];
  children?: React.ReactNode;
  onSelect: (value: string) => void | PromiseLike<void>;
  show: ShowMenu;
  onError: (error: unknown) => void;
};

function useMenuTrigger(props: TriggerProps) {
  const trigger = useRef<React.ElementRef<typeof Pressable>>(null);
  const pending = useRef(false);
  const mounted = useRef(false);
  const current = useRef(props);
  current.current = props;
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);

  const { component, items, selectedValue, show, onError, disabled } = props;
  const unavailable = disabled || !Array.isArray(items) || !items.some((item) => isGuestMenuOption(item) && !item.disabled);
  function open() {
    if (pending.current || unavailable || !trigger.current) return;
    pending.current = true;
    // Fabric's page coordinates are relative to the renderer's React Native root.
    trigger.current.measure((_x, _y, width, height, x, y) => {
      if (!mounted.current || ![x, y, width, height].every(Number.isFinite) || width <= 0 || height <= 0) {
        pending.current = false;
        return;
      }
      void completeGuestMenu(
        show({ component, items, selectedValue, anchor: { x, y, width, height } }),
        () => { pending.current = false; },
        (value) => {
          const latest = current.current;
          if (mounted.current && !latest.disabled && Array.isArray(latest.items) &&
              latest.items.some((item) => isGuestMenuOption(item) && item.value === value && !item.disabled)) {
            return latest.onSelect(value);
          }
        },
        onError,
      );
    });
  }
  return { trigger, unavailable, open };
}

function MenuTrigger(props: TriggerProps) {
  const { component, title, accessibilityLabel, style, testID } = props;
  const { trigger, unavailable, open } = useMenuTrigger(props);
  const dark = useColorScheme() === 'dark';
  const select = component === 'Select';
  const color = dark ? '#f5f5f5' : '#0f1115';
  return (
    <Pressable ref={trigger} collapsable={false} accessibilityRole={select ? 'combobox' : 'button'}
      accessibilityLabel={accessibilityLabel ?? title} accessibilityValue={select ? { text: title } : undefined}
      disabled={unavailable} testID={testID} onPress={open}
      style={[{
        minHeight: select ? 36 : 30, flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between',
        paddingHorizontal: select ? 15 : 8, borderRadius: select ? 18 : 8,
        backgroundColor: select ? (dark ? '#353638' : '#f1f3f5') : 'transparent',
        opacity: unavailable ? 0.5 : 1,
      }, style as ViewStyle]}
    >
      {props.children ?? <Text numberOfLines={1} style={{ color, fontSize: select ? 14 : 17, lineHeight: select ? 20 : undefined, flexShrink: 1 }}>
        {title}
      </Text>}
      {select ? <SelectChevron color={dark ? '#b3b7bd' : '#62666b'} /> : null}
    </Pressable>
  );
}

function SelectChevron({ color }: { color: string }) {
  return (
    <View pointerEvents="none" style={{
      width: 12, height: 16, marginLeft: 8, flexShrink: 0, alignItems: 'center', justifyContent: 'center',
    }}>
      {Svg ? <Svg width={14} height={14} viewBox="0 0 14 14"
        paths={[{ d: 'M3 5.5 L7 9.5 L11 5.5', fill: 'none', stroke: color, strokeWidth: 1.25,
          strokeLinecap: 'round', strokeLinejoin: 'round' }]} />
        : <Text style={{ color, lineHeight: 16 }}>⌄</Text>}
    </View>
  );
}

export function createGuestMenuComponents(tabId: string, reloadKey: number, onError: (error: unknown) => void) {
  const bridge = NativeModules.GuestMenu as NativeGuestMenu | undefined;
  if (typeof bridge?.show !== 'function') return undefined;
  const show: ShowMenu = async ({ component, anchor, items, selectedValue }) => {
    const entries = guestMenuEntries(items);
    return bridge.show(tabId, reloadKey, JSON.stringify({ component, anchor, items: entries, selectedValue }));
  };

  return {
    Select({ options, value, placeholder, onValueChange, ...props }: SelectProps) {
      const selected = Array.isArray(options)
        ? options.find((option) => isGuestMenuOption(option) && option.value === value)
        : undefined;
      return <MenuTrigger {...props} component="Select" items={options} selectedValue={selected?.value}
        title={selected?.label ?? placeholder ?? ''} onSelect={onValueChange} show={show} onError={onError} />;
    },
    Menu({ items, onSelect, children, ...props }: MenuProps) {
      return <MenuTrigger {...props} component="Menu" items={items} onSelect={onSelect} show={show} onError={onError}>
        {children as React.ReactNode}
      </MenuTrigger>;
    },
  };
}
