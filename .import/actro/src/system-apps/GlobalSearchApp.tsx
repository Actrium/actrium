/**
 * Global Search App - 全局搜索页面
 */
import React, { useState } from 'react';
import {
  View,
  Text,
  TextInput,
  ScrollView,
  StyleSheet,
  Image,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { MyTouchableOpacity } from 'askit';
import { Icon } from '../actr-host/components/Icon';

interface SearchItem {
  id: string;
  name: string;
  logo?: string;
}

interface SearchGroup {
  title: string;
  items: SearchItem[];
}

interface GlobalSearchAppProps {
  onClose?: () => void;
}

export default function GlobalSearchApp({ onClose }: GlobalSearchAppProps) {
  const { t } = useTranslation();
  const [searchQuery, setSearchQuery] = useState('');

  // TODO: 替换为实际搜索逻辑
  const searchGroups: SearchGroup[] = [
    {
      title: t('actr.search.active', '活跃'),
      items: [
        { id: '1', name: 'name', logo: 'https://images.unsplash.com/photo-1606216794074-735e91aa2c92?w=100' },
        { id: '2', name: 'name', logo: 'https://images.unsplash.com/photo-1606216794074-735e91aa2c92?w=100' },
        { id: '3', name: 'name', logo: 'https://images.unsplash.com/photo-1606216794074-735e91aa2c92?w=100' },
      ],
    },
    {
      title: t('actr.search.bookmarks', '书签'),
      items: [
        { id: '4', name: 'name', logo: 'https://images.unsplash.com/photo-1606216794074-735e91aa2c92?w=100' },
        { id: '5', name: 'name', logo: 'https://images.unsplash.com/photo-1606216794074-735e91aa2c92?w=100' },
      ],
    },
    {
      title: t('actr.search.settings', '设置'),
      items: [
        { id: '6', name: 'namename' },
      ],
    },
  ];

  const settingsItems = [
    { id: 'appearance', name: t('actr.search.appearance', '外观设置') },
  ];

  const handleClearSearch = () => {
    setSearchQuery('');
  };

  return (
    <View style={styles.container}>
      {/* 标题 */}
      <View style={styles.header}>
        <Text style={styles.headerTitle}>
          {t('actr.search.resultsIn', '在书签内搜索到')}
        </Text>
      </View>

      {/* 搜索结果列表 */}
      <ScrollView
        style={styles.content}
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        {searchGroups.map((group, groupIndex) => (
          <View key={groupIndex} style={styles.groupContainer}>
            <Text style={styles.groupTitle}>{group.title}</Text>
            <View style={styles.groupCard}>
              {group.items.map((item, index) => (
                <React.Fragment key={item.id}>
                  <MyTouchableOpacity style={styles.listItem}>
                    <Image
                      source={{ uri: item.logo }}
                      style={styles.listItemLogo}
                    />
                    <Text style={styles.listItemName} numberOfLines={1}>
                      {item.name}
                    </Text>
                  </MyTouchableOpacity>
                  {index < group.items.length - 1 && (
                    <View style={styles.listItemDivider} />
                  )}
                </React.Fragment>
              ))}
            </View>
          </View>
        ))}

        {/* 设置项 */}
        {settingsItems.map((item) => (
          <MyTouchableOpacity key={item.id} style={styles.settingsItem}>
            <Text style={styles.settingsIcon}>🌐</Text>
            <Text style={styles.settingsItemName}>{item.name}</Text>
          </MyTouchableOpacity>
        ))}
      </ScrollView>

      {/* 底部搜索框 */}
      <View style={styles.searchBarContainer}>
        <View style={styles.searchBar}>
          <Icon type="link" size={20} color="#8E8E93" style={styles.searchIcon} />
          <TextInput
            style={styles.searchInput}
            placeholder={t('actr.search.placeholder', 'Search Term')}
            placeholderTextColor="#8E8E93"
            value={searchQuery}
            onChangeText={setSearchQuery}
          />
          {searchQuery.length > 0 && (
            <MyTouchableOpacity onPress={handleClearSearch}>
              <Icon type="input_clear" size={18} color="#8E8E93" />
            </MyTouchableOpacity>
          )}
        </View>
        <MyTouchableOpacity style={styles.closeButton} onPress={onClose}>
          <Icon type="close" size={16} color="#8E8E93" />
        </MyTouchableOpacity>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#000',
  },
  header: {
    paddingHorizontal: 24,
    paddingTop: 60,
    paddingBottom: 16,
  },
  headerTitle: {
    fontSize: 20,
    fontWeight: '500',
    color: '#fff',
  },
  content: {
    flex: 1,
  },
  scrollContent: {
    paddingHorizontal: 24,
    paddingBottom: 100,
  },
  groupContainer: {
    marginBottom: 16,
  },
  groupTitle: {
    fontSize: 13,
    fontWeight: '400',
    color: '#8E8E93',
    marginBottom: 8,
    marginLeft: 4,
  },
  groupCard: {
    borderRadius: 16,
    backgroundColor: '#1C1C1E',
    overflow: 'hidden',
  },
  listItem: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 16,
    height: 52,
  },
  listItemDivider: {
    height: 1,
    backgroundColor: '#2C2C2E',
    marginLeft: 60,
  },
  listItemLogo: {
    width: 32,
    height: 32,
    borderRadius: 16,
    backgroundColor: '#2C2C2E',
  },
  listItemName: {
    flex: 1,
    fontSize: 17,
    color: '#fff',
    marginLeft: 12,
    fontWeight: '400',
  },
  settingsItem: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: 12,
    paddingHorizontal: 4,
  },
  settingsIcon: {
    fontSize: 20,
    marginRight: 12,
  },
  settingsItemName: {
    fontSize: 17,
    color: '#fff',
    fontWeight: '400',
  },
  searchBarContainer: {
    position: 'absolute',
    bottom: 0,
    left: 0,
    right: 0,
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: 16,
    paddingVertical: 12,
    paddingBottom: 34,
    backgroundColor: '#000',
  },
  searchBar: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#1C1C1E',
    borderRadius: 12,
    paddingHorizontal: 12,
    height: 44,
  },
  searchIcon: {
    marginRight: 8,
  },
  searchInput: {
    flex: 1,
    fontSize: 17,
    color: '#fff',
  },
  closeButton: {
    width: 44,
    height: 44,
    borderRadius: 22,
    backgroundColor: '#1C1C1E',
    alignItems: 'center',
    justifyContent: 'center',
    marginLeft: 12,
  },
});
