import React from 'react';
import { Image, Pressable, Text, View } from 'react-native';

/** Displays an original whose URL came from the native attachment cache. */
export function CachedImagePreview({ uri, close }: { uri: string; close: () => void }) {
  const [failed, setFailed] = React.useState(false);
  return (
    <View style={{
      position: 'absolute', top: 0, right: 0, bottom: 0, left: 0,
      backgroundColor: '#15171b', padding: 16, zIndex: 100,
    }} accessibilityViewIsModal>
      <Pressable accessibilityRole="button" accessibilityLabel="Close preview" onPress={close}
        style={{ alignSelf: 'flex-end', paddingHorizontal: 16, paddingVertical: 8 }}>
        <Text style={{ color: '#fff', fontSize: 16 }}>Close</Text>
      </Pressable>
      {failed ? <Text style={{ color: '#fff' }}>Could not load the original image.</Text> : (
        <Image source={{ uri }} resizeMode="contain" style={{ flex: 1, width: '100%' }}
          accessibilityLabel="Original image" onError={() => setFailed(true)} />
      )}
    </View>
  );
}
