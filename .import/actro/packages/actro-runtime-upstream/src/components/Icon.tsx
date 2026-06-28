import React from 'react';
import { Image, StyleSheet } from 'react-native';
import { hostIconGlyphSize } from '../theme/tokens';
import { useColors } from '../theme/useColors';

const IconSources: Record<string, any> = {
  extension_menu: require('../assets/icons/extension_menu.png'),
  close: require('../assets/icons/close.png'),
  link: require('../assets/icons/link.png'),
  network: require('../assets/icons/network.png'),
  network_active: require('../assets/icons/network_active.png'),
  share_position: require('../assets/icons/share_position.png'),
  share_position_active: require('../assets/icons/share_position_active.png'),
  geolocation: require('../assets/icons/geolocation.png'),
  enter: require('../assets/icons/enter.png'),
  enter_active: require('../assets/icons/enter_active.png'),
  audio: require('../assets/icons/audio.png'),
  input_clear: require('../assets/icons/input_clear.png'),
  keyboard: require('../assets/icons/keyboard.png'),
  send: require('../assets/icons/send.png'),
  camera: require('../assets/icons/camera.png'),
  image: require('../assets/icons/photo.png'),
  file: require('../assets/icons/file.png'),
  copy: require('../assets/icons/copy.png'),
  more: require('../assets/icons/more.png'),
  tab_preview: require('../assets/icons/tab_preview.png'),
  menu_more: require('../assets/icons/menu_more.png'),
  voice: require('../assets/icons/voice.png'),
  play: require('../assets/icons/voice.png'),
  edit: require('../assets/icons/edit.png'),
  delete: require('../assets/icons/delete.png'),
  feedback: require('../assets/icons/feedback.png'),
  suggest:  require('../assets/icons/suggest.png'),
  next: require('../assets/icons/next.png'),
  logout: require('../assets/icons/logout.png'),
  search: require('../assets/icons/search.png'),
  askaway_gery: require('../assets/icons/askaway_gery.png'),
  user_item: require('../assets/icons/user_item.png'),
  user_select: require('../assets/icons/user_select.png'),
  history: require('../assets/icons/history.png'),
  remove_star: require('../assets/icons/remove_star.png'),
  location_pin: require('../assets/icons/location_pin.png'),
  share: require('../assets/icons/share.png'),
  success: require('../assets/icons/success.png'),
  bookmark_empty: require('../assets/icons/bookmark-empty.png'),
  link_pad: require('../assets/icons/link-pad.png'),
  audio_pad: require('../assets/icons/audio-pad.png'),
};

interface IconProps {
  type: string;
  active?: boolean;
  size?: number;
  color?: string;
  style?: any;
  disableTint?: boolean;
}

/**
 * 优化后的 Icon 组件 (iPad 性能增强版)
 */
export const Icon: React.FC<IconProps> = React.memo(
  ({
    type,
    active = false,
    size = hostIconGlyphSize.sm,
    color,
    style,
    disableTint = false,
  }) => {
    const colors = useColors();
    const finalColor = color ?? colors.iconDefault;
    const activeType = `${type}_active`;
    const activeSource = IconSources[activeType];
    const source = active ? activeSource || IconSources[type] : IconSources[type];
    const shouldApplyTint = !disableTint && !(active && activeSource);
    if (!source) return null;

    return (
      <Image
        source={source}
        style={[
          {
            height: size,
            width: size,
            tintColor: shouldApplyTint ? finalColor : undefined,
          },
          style,
        ]}
        resizeMode='contain'
      />
    );
  },
);
