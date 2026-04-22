import React, {
  useState,
  useRef,
  useEffect,
  useCallback,
} from 'react';
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
import { HOST_SHADOWS } from '../../actr-host/styles/shadows';
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
  const { keyboardInset, isKeyboardVisible, dismissKeyboard } = useKeyboardInset();

  const inputWidth = 500;
  const inputBorderRadius = 24;
  const containerGap = 36;
  const shouldApplyKeyboardInset = Platform.OS === 'ios';

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

    // 中文注释：Pad 端退出搜索前先给键盘一点收起时间，
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
      <ScrollView
        contentContainerStyle={[
          styles.scrollContent,
          { paddingBottom: shouldApplyKeyboardInset ? keyboardInset : 0 },
        ]}
        keyboardShouldPersistTaps='handled'
      >
        <View style={[styles.GlobalSearchContainer, { gap: containerGap }]}>
          <View
            style={[
              styles.resultsArea,
              {
                // 中文注释：搜索结果列表横向边距对齐 settings 一级页，
                // Pad 用 40，不再额外缩成 500 宽。
                width: Math.max(screenWidth - resultsHorizontalPadding * 2, 0),
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

          {/* 中文注释：Pad 端输入区维持原来的底部锚定布局，
              避免结果为空时输入栏跟随内容一起漂到中间。 */}
          <View style={[styles.padInputSection, { width: inputWidth }]}>
            <View style={styles.searchRow}>
              <View
                style={[
                  styles.inputContainer,
                  { borderRadius: inputBorderRadius, backgroundColor: '#FFF' },
                ]}
              >
                <Icon
                  type='search'
                  size={globalSearch.inputPrefixIconSize}
                  color='#999'
                  style={styles.inputPrefixIcon}
                />
                <TextInput
                  ref={inputRef}
                  style={[
                    styles.input,
                    { borderRadius: inputBorderRadius },
                  ]}
                  placeholder={t('actr.search.placeholder')}
                  placeholderTextColor='#999'
                  value={inputText}
                  onChangeText={setInputText}
                  onSubmitEditing={handleSubmit}
                  returnKeyType='search'
                  autoFocus={true}
                  clearButtonMode='never'
                  keyboardAppearance='dark'
                />
                {inputText.length > 0 && (
                  <HostIconButton
                    iconType='input_clear'
                    size={globalSearch.clearButtonSize}
                    iconSize={globalSearch.clearButtonIconSize}
                    iconColor='#666'
                    backgroundColor='transparent'
                    style={styles.clearButton}
                    onPress={handleClearSearch}
                  />
                )}
              </View>
              <HostIconButton
                iconType='close'
                size={globalSearch.closeButtonSize}
                iconSize={globalSearch.closeButtonIconSize}
                iconColor='#404040'
                backgroundColor='#FFF'
                style={styles.standaloneCloseButton}
                onPress={handleCloseSearch}
              />
            </View>
          </View>
        </View>
      </ScrollView>
    </TouchableWithoutFeedback>
  );
}

const styles = StyleSheet.create({
  scrollContent: {
    flexGrow: 1,
  },
  GlobalSearchContainer: {
    flex: 1,
    alignItems: 'center',
    flexDirection: 'column',
    paddingTop: 40,
    paddingBottom: 16,
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
    marginTop: 'auto',
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
    ...HOST_SHADOWS.inputPill,
    flexDirection: 'row',
    alignItems: 'center',
  },
  inputPrefixIcon: {
    marginLeft: globalSearch.inputPrefixMarginLeft,
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
    ...HOST_SHADOWS.inputPill,
  },
});
