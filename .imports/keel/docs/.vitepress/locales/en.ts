import type { DefaultTheme, LocaleSpecificConfig } from 'vitepress';

const editPattern = ({ filePath }: { filePath: string }) =>
  `https://github.com/Actrium/keel/edit/main/docs/${filePath}`;

export const enLocale: LocaleSpecificConfig<DefaultTheme.Config> & {
  label: string;
  lang: string;
} = {
  label: 'English',
  lang: 'en-US',
  description: 'Lightweight, headless, sandboxed dynamic UI rendering engine for React Native.',
  themeConfig: {
    nav: [
      { text: 'Get Started', link: '/getting-started/' },
      { text: 'Guides', link: '/guides/' },
      { text: 'API', link: '/api/' },
      { text: 'Architecture', link: '/architecture/' },
      { text: 'Reference', link: '/reference/' },
    ],
    sidebar: [
      {
        text: 'Getting Started',
        collapsed: false,
        items: [
          { text: 'Quick Start', link: '/getting-started/' },
          { text: 'Host Integration', link: '/getting-started/host-integration' },
          { text: 'Guest Development', link: '/getting-started/guest-development' },
        ],
      },
      {
        text: 'Guides',
        collapsed: false,
        items: [
          { text: 'Overview', link: '/guides/' },
          { text: 'CLI', link: '/guides/cli' },
          { text: 'Sandbox Providers', link: '/guides/sandbox-providers' },
          { text: 'Native Integration', link: '/guides/native-integration' },
          { text: 'Multi-Tenant', link: '/guides/multi-tenant' },
          { text: 'Production', link: '/guides/production' },
          { text: 'Host Module Types', link: '/guides/host-module-types' },
          { text: 'Migrating from rill', link: '/guides/migrating-from-rill' },
        ],
      },
      {
        text: 'API',
        collapsed: false,
        items: [
          { text: 'Overview', link: '/api/' },
          { text: 'Host API', link: '/api/host' },
          { text: 'Guest SDK', link: '/api/guest' },
          { text: 'TenantManager API', link: '/api/tenant-manager' },
        ],
      },
      {
        text: 'Architecture',
        collapsed: true,
        items: [
          { text: 'Overview', link: '/architecture/' },
          { text: 'Guest-Host Interaction', link: '/architecture/guest-host-interaction' },
          { text: 'Guest Runtime', link: '/architecture/guest-runtime' },
          { text: 'Bridge Serialization', link: '/architecture/bridge-serialization' },
          { text: 'Native TenantManager', link: '/architecture/native-tenant-manager' },
          { text: 'Binary Protocol', link: '/architecture/binary-protocol' },
          { text: 'Security', link: '/architecture/security' },
          { text: 'Event Bus', link: '/architecture/event-bus' },
          { text: 'CDP Debugging', link: '/architecture/cdp-debugging' },
        ],
      },
      {
        text: 'Reference',
        collapsed: true,
        items: [
          { text: 'Overview', link: '/reference/' },
          { text: 'Sandbox Comparison', link: '/reference/sandbox-comparison' },
          { text: 'Guest Lines (JS vs native)', link: '/reference/guest-lines' },
        ],
      },
    ],
    editLink: {
      pattern: editPattern,
      text: 'Edit this page on GitHub',
    },
    lastUpdated: { text: 'Last updated' },
    outline: { label: 'On this page' },
    docFooter: { prev: 'Previous', next: 'Next' },
    returnToTopLabel: 'Return to top',
    sidebarMenuLabel: 'Menu',
    darkModeSwitchLabel: 'Appearance',
    footer: {
      message: 'Released under the Apache-2.0 License.',
      copyright: 'Copyright Actrium contributors',
    },
  },
};
