import React, { useMemo } from 'react';
import { NativeModules, Pressable, SectionList, StyleSheet, Text, View, useColorScheme } from 'react-native';
import { historySections, historySiteLabel } from './history-sections';
import type { BookmarkData, HistoryItemData, HistoryPageData } from './types';

type LibraryItem = HistoryItemData | BookmarkData;
type LibraryPageProps = { tabId: string } & (
  | { kind: 'history'; items: readonly HistoryItemData[]; page?: HistoryPageData }
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

type ItemActions = {
  open?: (id: string) => void;
  remove?: (id: string) => void;
  setPage?: (index: number) => void;
};

function itemActions(kind: LibraryPageProps['kind'], tabId: string): ItemActions {
  if (kind === 'history') {
    const native = NativeModules.HistoryModule;
    if (!native) return {};
    return {
      open: (id: string) => native.openHistoryItem(tabId, id),
      remove: (id: string) => native.deleteHistoryItem(tabId, id),
      setPage: typeof native.setHistoryPage === 'function'
        ? (index: number) => native.setHistoryPage(tabId, index)
        : undefined,
    };
  }
  const native = NativeModules.BookmarksModule;
  if (!native) return {};
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
  const page = isHistory ? props.page : undefined;
  const emptyMessage = isHistory ? 'No history yet' : 'No bookmarks yet';
  const card = { backgroundColor: colors.background, borderColor: colors.border };
  const sections = useMemo(() => {
    if (kind === 'history') return historySections(items);
    return items.length ? [{ key: 'bookmarks', data: items }] : [];
  }, [kind, items]);
  const actions = itemActions(kind, tabId);

  return (
    <SectionList<LibraryItem, { title?: string }>
      key={page?.index}
      style={[styles.page, { backgroundColor: colors.pageBackground }]}
      contentContainerStyle={styles.content}
      sections={sections}
      keyExtractor={item => item.id}
      stickySectionHeadersEnabled={false}
      ListHeaderComponent={
        <View style={styles.pageHeader}>
          <View accessible accessibilityRole="header" accessibilityLabel={title}>
            <Text style={[styles.title, { color: colors.text }]}>{title}</Text>
          </View>
          {page && page.count > 1 && actions.setPage ? (
            <HistoryPageControls page={page} setPage={actions.setPage} colors={colors} />
          ) : null}
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

function HistoryPageControls({
  page, setPage, colors,
}: { page: HistoryPageData; setPage: (index: number) => void; colors: typeof light }) {
  const newerAvailable = page.index > 0;
  const olderAvailable = page.index + 1 < page.count;
  return (
    <View style={styles.pageControls}>
      <Pressable accessibilityRole="button" accessibilityLabel="Newer history" disabled={!newerAvailable}
        onPress={() => setPage(page.index - 1)} style={!newerAvailable && styles.disabled}>
        <Text style={{ color: colors.text }}>Newer</Text>
      </Pressable>
      <Text style={{ color: colors.secondary }}>{page.index + 1} / {page.count}</Text>
      <Pressable accessibilityRole="button" accessibilityLabel="Older history" disabled={!olderAvailable}
        onPress={() => setPage(page.index + 1)} style={!olderAvailable && styles.disabled}>
        <Text style={{ color: colors.text }}>Older</Text>
      </Pressable>
    </View>
  );
}

type RowProps = { colors: typeof light; open?: (id: string) => void; remove?: (id: string) => void };

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
        disabled={!address || !open}
        onPress={open && (() => open(item.id))}
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
      <RemoveButton title={item.title} colors={colors} onPress={remove && (() => remove(item.id))} subdued={isVisit} />
    </View>
  );
}

function RemoveButton({
  title, colors, onPress, subdued = false,
}: { title: string; colors: typeof light; onPress?: () => void; subdued?: boolean }) {
  return (
    <Pressable
      accessibilityLabel={`Remove ${title}`}
      accessibilityRole="button"
      disabled={!onPress}
      onPress={onPress}
      style={({ pressed }) => [styles.remove, !onPress && styles.disabled, pressed && { backgroundColor: colors.pressed }]}
    >
      <Text style={{ color: subdued ? colors.secondary : colors.remove }}>Remove</Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  page: { flex: 1 },
  content: { width: '100%', maxWidth: 1120, alignSelf: 'center', paddingHorizontal: 24, paddingVertical: 24 },
  pageHeader: { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', marginBottom: 18 },
  title: { fontSize: 26, fontWeight: '700' },
  pageControls: { flexDirection: 'row', alignItems: 'center', gap: 12 },
  disabled: { opacity: 0.45 },
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
