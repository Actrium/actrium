import React, { useMemo } from 'react';
import { Image, NativeModules, Pressable, SectionList, StyleSheet, Text, View, useColorScheme } from 'react-native';
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
      renderItem={({ item, index, section }) => (
        <LibraryRow
          item={item}
          colors={colors}
          open={actions.open}
          remove={actions.remove}
          showDivider={index < section.data.length - 1}
        />
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

function LibraryRow({
  item, colors, open, remove, showDivider,
}: RowProps & { item: LibraryItem; showDivider: boolean }) {
  const visitTime = 'visitTime' in item ? item.visitTime : undefined;
  const isVisit = visitTime !== undefined;
  const address = item.appURL;
  return (
    <View
      style={[
        styles.cardSides,
        styles.row,
        { backgroundColor: colors.background, borderColor: colors.border },
        showDivider && styles.rowDivider,
      ]}
    >
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
          <Text numberOfLines={1} ellipsizeMode="tail" style={[styles.itemTitle, { color: colors.text }]}>
            {item.title}
          </Text>
          {address ? (
            <Text numberOfLines={1} ellipsizeMode="middle" style={[styles.url, { color: colors.secondary }]}>
              {address}
            </Text>
          ) : null}
        </View>
      </Pressable>
      <RemoveButton title={item.title} colors={colors} onPress={remove && (() => remove(item.id))} />
    </View>
  );
}

/// Apple's SF Symbol "trash", pre-rendered at 3x in the two secondary grays.
/// The runtime bundle has no icon font or SF Symbol component, so the glyphs
/// travel with the bundle as data URIs.
const TRASH_ICON_URI = {
  light: 'data:image/png;base64,' + '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',
  dark: 'data:image/png;base64,' + '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',
};

function TrashIcon() {
  const uri = useColorScheme() === 'dark' ? TRASH_ICON_URI.dark : TRASH_ICON_URI.light;
  return <Image source={{ uri }} style={styles.trashIcon} resizeMode="contain" />;
}

function RemoveButton({
  title, colors, onPress,
}: { title: string; colors: typeof light; onPress?: () => void }) {
  return (
    <Pressable
      accessibilityLabel={`Remove ${title}`}
      accessibilityRole="button"
      disabled={!onPress}
      onPress={onPress}
      style={({ pressed }) => [styles.remove, !onPress && styles.disabled, pressed && { backgroundColor: colors.pressed }]}
    >
      <TrashIcon />
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
  rowDivider: { borderBottomWidth: StyleSheet.hairlineWidth },
  time: { fontSize: 12, width: 76, fontVariant: ['tabular-nums'] },
  open: {
    flex: 1, minWidth: 0, flexDirection: 'row', alignItems: 'center',
    gap: 8, borderRadius: 6, paddingHorizontal: 4, paddingVertical: 6,
  },
  // Title and address share one line. The title is never shortened — the
  // address gives up width first and ellipsizes in the middle so both its
  // head and tail stay readable.
  itemDetails: { flex: 1, minWidth: 0, flexDirection: 'row', alignItems: 'center', gap: 8 },
  itemTitle: { fontSize: 14, fontWeight: '500', flexShrink: 0 },
  url: { fontSize: 12, flexGrow: 1, flexShrink: 1, minWidth: 0 },
  siteIcon: { width: 17, height: 17, borderRadius: 4, alignItems: 'center', justifyContent: 'center' },
  initial: { fontSize: 11, fontWeight: '600' },
  remove: { marginLeft: 8, paddingHorizontal: 8, paddingVertical: 8, borderRadius: 5 },
  trashIcon: { width: 14, height: 14 },
});
