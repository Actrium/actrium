import React, { useMemo } from 'react';
import { NativeModules, Pressable, SectionList, StyleSheet, Text, View, useColorScheme } from 'react-native';
import { historySections, historySiteLabel } from './history-sections';
import type { BookmarkData, HistoryItemData } from './types';

type LibraryItem = HistoryItemData | BookmarkData;
type LibraryPageProps = { tabId: string } & (
  | { kind: 'history'; items: readonly HistoryItemData[] }
  | { kind: 'bookmarks'; items: readonly BookmarkData[] }
);

const light = {
  background: '#ffffff', pageBackground: '#f6f8fa', text: '#222329', secondary: '#585960',
  border: '#e6e6e9', pressed: '#f0f0f3', remove: '#a02e3e',
};
const dark: typeof light = {
  background: '#202124', pageBackground: '#17181b', text: '#f1f1f4', secondary: '#b6b6bf',
  border: '#383941', pressed: '#34353c', remove: '#ff9ea8',
};

function itemActions(kind: LibraryPageProps['kind'], tabId: string) {
  if (kind === 'history') {
    const native = NativeModules.HistoryModule;
    return {
      open: (id: string) => native.openHistoryItem(tabId, id),
      remove: (id: string) => native.deleteHistoryItem(tabId, id),
    };
  }
  const native = NativeModules.BookmarksModule;
  return {
    open: (id: string) => native.openBookmark(tabId, id),
    remove: (id: string) => native.deleteBookmark(tabId, id),
  };
}

export function LibraryPage(props: LibraryPageProps) {
  const { kind, items, tabId } = props;
  const colors = useColorScheme() === 'dark' ? dark : light;
  const isHistory = kind === 'history';
  const title = isHistory ? 'History' : 'Bookmarks';
  const emptyMessage = isHistory ? 'No history yet' : 'No bookmarks yet';
  const card = { backgroundColor: colors.background, borderColor: colors.border };
  const sections = useMemo(() => {
    if (kind === 'history') return historySections(items);
    return items.length ? [{ key: 'bookmarks', data: items }] : [];
  }, [kind, items]);
  const actions = itemActions(kind, tabId);

  return (
    <SectionList<LibraryItem, { title?: string }>
      style={[styles.page, { backgroundColor: colors.pageBackground }]}
      contentContainerStyle={styles.content}
      sections={sections}
      keyExtractor={item => item.id}
      stickySectionHeadersEnabled={false}
      ListHeaderComponent={
        <View accessible accessibilityRole="header" accessibilityLabel={title}>
          <Text style={[styles.title, { color: colors.text }]}>{title}</Text>
        </View>
      }
      ListEmptyComponent={
        <View accessible accessibilityRole="text" accessibilityLabel={emptyMessage} style={[styles.emptyCard, card]}>
          <Text style={[styles.empty, { color: colors.secondary }]}>{emptyMessage}</Text>
        </View>
      }
      renderSectionHeader={({ section }) => (
        <View style={[styles.cardSides, styles.cardHeader, card]}>
          {section.title ? (
            <View accessible accessibilityRole="header" accessibilityLabel={section.title} style={styles.heading}>
              <Text style={[styles.section, { color: colors.text }]}>{section.title}</Text>
            </View>
          ) : null}
        </View>
      )}
      renderSectionFooter={() => <View style={[styles.cardSides, styles.cardFooter, card]} />}
      renderItem={({ item }) => (
        <LibraryRow item={item} colors={colors} open={actions.open} remove={actions.remove} />
      )}
    />
  );
}

type RowProps = { colors: typeof light; open: (id: string) => void; remove: (id: string) => void };

function LibraryRow({ item, colors, open, remove }: RowProps & { item: LibraryItem }) {
  const visitTime = 'visitTime' in item ? item.visitTime : undefined;
  const isVisit = visitTime !== undefined;
  const address = item.appURL;
  return (
    <View style={[styles.cardSides, styles.row, { backgroundColor: colors.background, borderColor: colors.border }]}>
      {isVisit ? (
        <Text style={[styles.time, { color: colors.secondary }]}>
          {new Date(visitTime).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })}
        </Text>
      ) : null}
      <Pressable
        accessibilityLabel={item.title}
        accessibilityHint={address}
        accessibilityRole="link"
        disabled={!address}
        onPress={() => open(item.id)}
        style={({ pressed }) => [styles.open, pressed && { backgroundColor: colors.pressed }]}
      >
        {isVisit ? (
          <View style={[styles.siteIcon, { backgroundColor: colors.pressed }]}>
            <Text style={[styles.initial, { color: colors.secondary }]}>
              {(historySiteLabel(address) ?? item.title).slice(0, 1).toLocaleUpperCase() || '·'}
            </Text>
          </View>
        ) : null}
        <View style={styles.itemDetails}>
          <Text numberOfLines={1} style={[styles.itemTitle, { color: colors.text }]}>{item.title}</Text>
          {address ? <Text style={[styles.url, { color: colors.secondary }]}>{address}</Text> : null}
        </View>
      </Pressable>
      <RemoveButton title={item.title} colors={colors} onPress={() => remove(item.id)} subdued={isVisit} />
    </View>
  );
}

function RemoveButton({
  title, colors, onPress, subdued = false,
}: { title: string; colors: typeof light; onPress: () => void; subdued?: boolean }) {
  return (
    <Pressable
      accessibilityLabel={`Remove ${title}`}
      accessibilityRole="button"
      onPress={onPress}
      style={({ pressed }) => [styles.remove, pressed && { backgroundColor: colors.pressed }]}
    >
      <Text style={{ color: subdued ? colors.secondary : colors.remove }}>Remove</Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1 },
  content: { width: '100%', maxWidth: 1120, alignSelf: 'center', paddingHorizontal: 24, paddingVertical: 24 },
  title: { fontSize: 26, fontWeight: '700', marginBottom: 18 },
  emptyCard: { borderWidth: StyleSheet.hairlineWidth, borderRadius: 12, paddingHorizontal: 22 },
  empty: { fontSize: 14, paddingVertical: 12 },
  cardSides: { borderLeftWidth: StyleSheet.hairlineWidth, borderRightWidth: StyleSheet.hairlineWidth },
  cardHeader: {
    borderTopWidth: StyleSheet.hairlineWidth, borderTopLeftRadius: 12, borderTopRightRadius: 12,
    paddingTop: 8,
  },
  cardFooter: {
    height: 9, marginBottom: 16, borderBottomWidth: StyleSheet.hairlineWidth,
    borderBottomLeftRadius: 12, borderBottomRightRadius: 12,
  },
  heading: { paddingHorizontal: 22, paddingTop: 13, paddingBottom: 12 },
  section: { fontSize: 16, fontWeight: '600' },
  row: { minHeight: 48, paddingHorizontal: 22, flexDirection: 'row', alignItems: 'center' },
  time: { fontSize: 12, width: 76, fontVariant: ['tabular-nums'] },
  open: {
    flex: 1, minWidth: 0, flexDirection: 'row', alignItems: 'center',
    gap: 8, borderRadius: 6, paddingHorizontal: 4, paddingVertical: 6,
  },
  itemDetails: { flex: 1, minWidth: 0 },
  itemTitle: { fontSize: 14, fontWeight: '500' },
  url: { fontSize: 12, marginTop: 3 },
  siteIcon: { width: 17, height: 17, borderRadius: 4, alignItems: 'center', justifyContent: 'center' },
  initial: { fontSize: 11, fontWeight: '600' },
  remove: { marginLeft: 8, paddingHorizontal: 8, paddingVertical: 8, borderRadius: 5 },
});
