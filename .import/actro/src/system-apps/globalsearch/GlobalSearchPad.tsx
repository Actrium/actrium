import React, { useState, useRef, useEffect, useCallback } from 'react';
import {
  View,
  TextInput,
  ScrollView,
  StyleSheet,
  TouchableWithoutFeedback,
  Platform,
} from 'react-native';
import { useTranslation } from 'react-i18next';
import { Icon, HostIconButton } from '../../components';
import { useKeyboardInset } from '../../hooks/useKeyboardInset';
import { getTypographyStyle, globalSearch } from '../../theme/tokens';
import { exitSearch } from '../../native-bridge/tab/TabModule';

interface GlobalSearchPadProps {
  renderResults: () => React.ReactNode;
  screenWidth: number;
  resultsHorizontalPadding: number;
  resultsAreaSize: number | 'auto';
  resultsCardRadius: number;
  onSearchQueryChange: (query: string) => void;
}

export default function GlobalSearchPad({
  renderResults,
  screenWidth,
  resultsHorizontalPadding,
  resultsAreaSize,
  resultsCardRadius,
  onSearchQueryChange,
}: GlobalSearchPadProps) {
  const { t } = useTranslation();
  const [inputText, setInputText] = useState('');
  const inputRef = useRef<TextInput>(null);
  const { keyboardInset, isKeyboardVisible, dismissKeyboard } =
    useKeyboardInset();

  const inputWidth = 500;
  const inputBorderRadius = 24;
  const containerGap = 36;
  const inputBottomOffset = 36;
  const keyboardGap = 12;
  const inputDockBottom = inputBottomOffset;
  const inputOccupiedHeight = globalSearch.inputHeight + inputBottomOffset;

  useEffect(() => {
    const timer = setTimeout(() => {
      inputRef.current?.focus();
    }, 200);
    return () => clearTimeout(timer);
  }, []);

  const handleSubmit = useCallback(() => {
    onSearchQueryChange(inputText.trim());
    dismissKeyboard();
  }, [inputText, onSearchQueryChange, dismissKeyboard]);

  const handleClearSearch = useCallback(() => {
    setInputText('');
    onSearchQueryChange('');
  }, [onSearchQueryChange]);

  const handleCloseSearch = useCallback(() => {
    dismissKeyboard();

    // Pad 端退出搜索前先给键盘一点收起时间，
    // 避免关闭页面和键盘动画同帧触发时出现宿主抢占导致的闪动。
    if (isKeyboardVisible) {
      setTimeout(() => {
        exitSearch();
      }, 180);
      return;
    }

    exitSearch();
  }, [dismissKeyboard, isKeyboardVisible]);

  return (
    <TouchableWithoutFeedback onPress={dismissKeyboard} accessible={false}>
      <View style={styles.padRoot}>
        <ScrollView
          contentContainerStyle={[
            styles.scrollContent,
            {
              // 内容区底部预留输入栏占位，
              // 避免底部搜索结果被绝对定位的输入栏遮住。
              paddingBottom: inputOccupiedHeight + keyboardGap,
            },
          ]}
          keyboardShouldPersistTaps="handled"
        >
          <View style={[styles.GlobalSearchContainer, { gap: containerGap }]}>
            <View
              style={[
                styles.resultsArea,
                {
                  // 搜索结果列表横向边距对齐 settings 一级页，
                  // Pad 用 40，不再额外缩成 500 宽。
                  width: Math.max(
                    screenWidth - resultsHorizontalPadding * 2,
                    0,
                  ),
                  height: resultsAreaSize,
                  borderRadius: resultsCardRadius,
                },
              ]}
            >
              <ScrollView
                style={{ flex: 1 }}
                contentContainerStyle={styles.resultsListContent}
                showsVerticalScrollIndicator={false}
              >
                {renderResults()}
              </ScrollView>
            </View>
          </View>
        </ScrollView>

        <View
          style={[
            styles.padInputDock,
            {
              // 当前页面所在宿主在 iOS 上已经会随键盘压缩可用高度，
              // 这里不再额外叠加 keyboardInset，避免输入栏被重复抬高。
              bottom: inputDockBottom,
            },
          ]}
        >
          {/* 中文注释：Pad 端输入区改为底部独立定位，
              避免继续受 ScrollView 自动滚动和 flex 收缩影响。 */}
          <View
            style={[
              styles.padInputSection,
              {
                width: inputWidth,
              },
            ]}
          >
            <View style={styles.searchRow}>
              <View
                style={[
                  styles.inputContainer,
                  { borderRadius: inputBorderRadius, backgroundColor: '#FFF' },
                ]}
              >
                <Icon
                  type="search"
                  size={globalSearch.inputPrefixIconSize}
                  color="#999"
                  style={styles.inputPrefixIcon}
                />
                <TextInput
                  ref={inputRef}
                  style={[styles.input, { borderRadius: inputBorderRadius }]}
                  placeholder={t('actr.search.placeholder')}
                  placeholderTextColor="#999"
                  value={inputText}
                  onChangeText={setInputText}
                  onSubmitEditing={handleSubmit}
                  returnKeyType="search"
                  autoFocus={true}
                  clearButtonMode="never"
                  keyboardAppearance="dark"
                />
                {inputText.length > 0 && (
                  <HostIconButton
                    iconType="input_clear"
                    size={globalSearch.clearButtonSize}
                    iconSize={globalSearch.clearButtonIconSize}
                    iconColor="#666"
                    backgroundColor="transparent"
                    style={styles.clearButton}
                    onPress={handleClearSearch}
                  />
                )}
              </View>
              <HostIconButton
                iconType="close"
                size={globalSearch.closeButtonSize}
                iconSize={globalSearch.closeButtonIconSize}
                iconColor="#404040"
                backgroundColor="#FFF"
                style={styles.standaloneCloseButton}
                onPress={handleCloseSearch}
              />
            </View>
          </View>
        </View>
      </View>
    </TouchableWithoutFeedback>
  );
}

const styles = StyleSheet.create({
  padRoot: {
    flex: 1,
  },
  scrollContent: {
    flexGrow: 1,
  },
  GlobalSearchContainer: {
    flex: 1,
    alignItems: 'center',
    flexDirection: 'column',
    paddingTop: 40,
  },
  resultsArea: {
    overflow: 'hidden',
    backgroundColor: 'transparent',
    borderRadius: 26,
  },
  resultsListContent: {
    paddingTop: globalSearch.resultsListPadding,
    paddingBottom: globalSearch.resultsListPadding,
  },
  padInputSection: {
    width: '100%',
  },
  searchRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: globalSearch.searchRowGap,
    width: '100%',
  },
  inputContainer: {
    flex: 1,
    position: 'relative',
    height: globalSearch.inputHeight,
    backgroundColor: '#FFF',
    flexDirection: 'row',
    alignItems: 'center',
  },
  inputPrefixIcon: {
    marginLeft: 16,
  },
  input: {
    flex: 1,
    paddingHorizontal: globalSearch.inputPaddingHorizontal,
    height: globalSearch.inputHeight,
    paddingVertical: 0,
    ...getTypographyStyle(globalSearch.inputTypographyVariant),
    color: '#000',
  },
  clearButton: {
    width: globalSearch.clearButtonSize,
    height: globalSearch.clearButtonSize,
    alignItems: 'center',
    justifyContent: 'center',
    marginRight: globalSearch.clearButtonMarginRight,
  },
  standaloneCloseButton: {
    width: globalSearch.closeButtonSize,
    height: globalSearch.closeButtonSize,
    borderRadius: globalSearch.closeButtonSize / 2,
    borderWidth: 1,
    borderColor: '#ECECEF',
    backgroundColor: '#FFF',
    alignItems: 'center',
    justifyContent: 'center',
  },
  padInputDock: {
    position: 'absolute',
    left: 0,
    right: 0,
    alignItems: 'center',
  },
});
