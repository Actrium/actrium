/**
 * Bookmark Tags System App - Collection Tabs Management Page
 */
import React, { useState, useEffect } from 'react';
import {
  View,
  Text,
  TextInput,
  ScrollView,
  StyleSheet,
  useWindowDimensions,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { MyTouchableOpacity } from 'askit';
import { GlobalAlert } from '../components';

interface Tag {
  id: string;
  name: string;
  selected: boolean;
}

export default function BookmarkTagsApp() {
  const { t } = useTranslation();
  const { width } = useWindowDimensions();
  const isTablet = width >= 768;

  const [tags, setTags] = useState<Tag[]>([
    { id: '1', name: 'tag', selected: true },
    { id: '2', name: 'tag', selected: false },
    { id: '3', name: 'tag', selected: false },
    { id: '4', name: 'tag', selected: false },
  ]);

  const [newTagName, setNewTagName] = useState('');
  const [isAddingTag, setIsAddingTag] = useState(false);

  const toggleTag = (id: string) => {
    setTags((prevTags) =>
      prevTags.map((tag) =>
        tag.id === id ? { ...tag, selected: !tag.selected } : tag
      )
    );
  };

  const handleAddTag = () => {
    if (newTagName.trim()) {
      const newTag: Tag = {
        id: Date.now().toString(),
        name: newTagName.trim(),
        selected: false,
      };
      setTags([...tags, newTag]);
      setNewTagName('');
      setIsAddingTag(false);
    }
  };

  const handleDeleteTag = (id: string) => {
    GlobalAlert.alert(
      t('actr.bookmarkTags.deleteTitle'),
      t('actr.bookmarkTags.deleteMessage'),
      [
        {
          text: t('common.cancel'),
          style: 'cancel',
        },
        {
          text: t('common.delete'),
          style: 'destructive',
          onPress: () => {
            setTags(tags.filter((tag) => tag.id !== id));
          },
        },
      ]
    );
  };

  return (
    <View style={styles.container}>
      <ScrollView
        style={styles.scrollView}
        contentContainerStyle={styles.scrollContent}
        showsVerticalScrollIndicator={false}
      >
        {/* Tab List */}
        <View style={styles.tagListContainer}>
          {tags.map((tag) => (
            <MyTouchableOpacity
              key={tag.id}
              style={styles.tagItem}
              onPress={() => toggleTag(tag.id)}
              onLongPress={() => handleDeleteTag(tag.id)}
              activeOpacity={0.7}
            >
              <View
                style={[
                  styles.checkbox,
                  tag.selected && styles.checkboxSelected,
                ]}
              >
                {tag.selected && (
                  <Text style={styles.checkmark}>✓</Text>
                )}
              </View>
              <Text style={styles.tagText}>{tag.name}</Text>
            </MyTouchableOpacity>
          ))}
        </View>

        {/* Add New Tab */}
        {isAddingTag ? (
          <View style={styles.addTagInputContainer}>
            <TextInput
              style={styles.addTagInput}
              placeholder={t('actr.bookmarkTags.enterTagName')}
              placeholderTextColor="#999"
              value={newTagName}
              onChangeText={setNewTagName}
              autoFocus
              onSubmitEditing={handleAddTag}
              onBlur={() => {
                if (!newTagName.trim()) {
                  setIsAddingTag(false);
                }
              }}
            />
            <MyTouchableOpacity
              style={styles.addTagConfirmButton}
              onPress={handleAddTag}
            >
              <Text style={styles.addTagConfirmText}>
                {t('common.confirm')}
              </Text>
            </MyTouchableOpacity>
          </View>
        ) : (
          <MyTouchableOpacity
            style={styles.addButton}
            onPress={() => setIsAddingTag(true)}
            activeOpacity={0.7}
          >
            <View style={styles.addIcon}>
              <Text style={styles.addIconText}>+</Text>
            </View>
            <Text style={styles.addButtonText}>
              {t('actr.bookmarkTags.addNewTag')}
            </Text>
          </MyTouchableOpacity>
        )}
      </ScrollView>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#F2F2F7',
  },
  scrollView: {
    flex: 1,
  },
  scrollContent: {
    padding: 16,
  },
  tagListContainer: {
    backgroundColor: '#FFFFFF',
    borderRadius: 16,
    overflow: 'hidden',
    marginBottom: 16,
  },
  tagItem: {
    flexDirection: 'row',
    alignItems: 'center',
    paddingVertical: 16,
    paddingHorizontal: 16,
    borderBottomWidth: 1,
    borderBottomColor: '#E5E5EA',
  },
  checkbox: {
    width: 24,
    height: 24,
    borderRadius: 12,
    borderWidth: 2,
    borderColor: '#C7C7CC',
    alignItems: 'center',
    justifyContent: 'center',
    marginRight: 12,
  },
  checkboxSelected: {
    backgroundColor: '#007AFF',
    borderColor: '#007AFF',
  },
  checkmark: {
    color: '#FFFFFF',
    fontSize: 16,
    fontWeight: '600',
  },
  tagText: {
    fontSize: 17,
    color: '#000000',
    flex: 1,
  },
  addButton: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#FFFFFF',
    borderRadius: 16,
    paddingVertical: 16,
    paddingHorizontal: 16,
  },
  addIcon: {
    width: 24,
    height: 24,
    borderRadius: 12,
    backgroundColor: '#007AFF',
    alignItems: 'center',
    justifyContent: 'center',
    marginRight: 12,
  },
  addIconText: {
    color: '#FFFFFF',
    fontSize: 20,
    fontWeight: '600',
  },
  addButtonText: {
    fontSize: 17,
    color: '#000000',
  },
  addTagInputContainer: {
    flexDirection: 'row',
    alignItems: 'center',
    backgroundColor: '#FFFFFF',
    borderRadius: 16,
    paddingVertical: 12,
    paddingHorizontal: 16,
    gap: 12,
  },
  addTagInput: {
    flex: 1,
    fontSize: 17,
    color: '#000000',
    paddingVertical: 4,
  },
  addTagConfirmButton: {
    paddingHorizontal: 16,
    paddingVertical: 8,
    backgroundColor: '#007AFF',
    borderRadius: 8,
  },
  addTagConfirmText: {
    color: '#FFFFFF',
    fontSize: 15,
    fontWeight: '600',
  },
});
