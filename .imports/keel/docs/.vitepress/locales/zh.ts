import type { DefaultTheme, LocaleSpecificConfig } from 'vitepress';

const editPattern = ({ filePath }: { filePath: string }) =>
  `https://github.com/Actrium/keel/edit/main/docs/${filePath}`;

export const zhSearch = {
  translations: {
    button: {
      buttonText: '搜索',
      buttonAriaLabel: '搜索',
    },
    modal: {
      displayDetails: '显示详细列表',
      resetButtonTitle: '重置搜索',
      backButtonTitle: '关闭搜索',
      noResultsText: '没有结果',
      footer: {
        selectText: '选择',
        navigateText: '切换',
        closeText: '关闭',
      },
    },
  },
};

export const zhLocale: LocaleSpecificConfig<DefaultTheme.Config> & {
  label: string;
  lang: string;
  markdown?: {
    container?: Record<string, string>;
  };
} = {
  label: '中文',
  lang: 'zh-CN',
  description: '轻量、无界面、沙箱化的 React Native 动态 UI 渲染引擎。',
  markdown: {
    container: {
      tipLabel: '提示',
      warningLabel: '注意',
      dangerLabel: '警告',
      infoLabel: '信息',
      detailsLabel: '详细信息',
    },
  },
  themeConfig: {
    nav: [
      { text: '快速开始', link: '/zh/getting-started/' },
      { text: '指南', link: '/zh/guides/' },
      { text: 'API', link: '/zh/api/' },
      { text: '架构', link: '/zh/architecture/' },
      { text: '参考', link: '/zh/reference/' },
    ],
    sidebar: [
      {
        text: '快速开始',
        collapsed: false,
        items: [
          { text: '快速开始', link: '/zh/getting-started/' },
          { text: '宿主集成', link: '/zh/getting-started/host-integration' },
          { text: '访客开发', link: '/zh/getting-started/guest-development' },
        ],
      },
      {
        text: '指南',
        collapsed: false,
        items: [
          { text: '概览', link: '/zh/guides/' },
          { text: 'CLI', link: '/zh/guides/cli' },
          { text: '沙箱提供者', link: '/zh/guides/sandbox-providers' },
          { text: '原生集成', link: '/zh/guides/native-integration' },
          { text: '多租户', link: '/zh/guides/multi-tenant' },
          { text: '生产环境', link: '/zh/guides/production' },
          { text: 'Host 模块类型', link: '/zh/guides/host-module-types' },
          { text: '从 rill 迁移', link: '/zh/guides/migrating-from-rill' },
        ],
      },
      {
        text: 'API',
        collapsed: false,
        items: [
          { text: '概览', link: '/zh/api/' },
          { text: 'Host API', link: '/zh/api/host' },
          { text: 'Guest SDK', link: '/zh/api/guest' },
          { text: 'TenantManager API', link: '/zh/api/tenant-manager' },
        ],
      },
      {
        text: '架构',
        collapsed: true,
        items: [
          { text: '概览', link: '/zh/architecture/' },
          { text: '访客-宿主交互', link: '/zh/architecture/guest-host-interaction' },
          { text: '访客运行时', link: '/zh/architecture/guest-runtime' },
          { text: '桥接序列化', link: '/zh/architecture/bridge-serialization' },
          { text: '原生编排器', link: '/zh/architecture/native-tenant-manager' },
          { text: '二进制协议', link: '/zh/architecture/binary-protocol' },
          { text: '安全性', link: '/zh/architecture/security' },
          { text: '事件总线', link: '/zh/architecture/event-bus' },
          { text: 'CDP 调试', link: '/zh/architecture/cdp-debugging' },
        ],
      },
      {
        text: '参考',
        collapsed: true,
        items: [
          { text: '概览', link: '/zh/reference/' },
          { text: '沙箱对比', link: '/zh/reference/sandbox-comparison' },
          { text: '访客线路（JS 与原生）', link: '/zh/reference/guest-lines' },
        ],
      },
    ],
    editLink: {
      pattern: editPattern,
      text: '在 GitHub 上编辑此页',
    },
    lastUpdated: { text: '上次更新' },
    outline: { label: '本页目录' },
    docFooter: { prev: '上一页', next: '下一页' },
    returnToTopLabel: '返回顶部',
    sidebarMenuLabel: '菜单',
    darkModeSwitchLabel: '外观',
    footer: {
      message: '以 Apache-2.0 许可发布。',
      copyright: 'Copyright Actrium contributors',
    },
  },
};
