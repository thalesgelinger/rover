import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';
import {roverLightTheme} from './src/data/prism-rover-light';
import {roverDarkTheme} from './src/data/prism-rover-dark';

// This runs in Node.js - Don't use client-side code here (browser APIs, JSX...)

const config: Config = {
  title: 'Rover',
  tagline: 'Lua runtime for building REAL full-stack applications - web, mobile, desktop, and backends',
  favicon: 'img/rover-icon.svg',

  // Future flags, see https://docusaurus.io/docs/api/docusaurus-config#future
  future: {
    v4: true, // Improve compatibility with the upcoming Docusaurus v4
  },

  // Set the production url of your site here
  url: 'https://your-domain.pages.dev', // Update with actual Cloudflare domain after deploy
  // Set the /<baseUrl>/ pathname under which your site is served
  baseUrl: '/',

  onBrokenLinks: 'throw',

  // Even if you don't use internationalization, you can use this field to set
  // useful metadata like html lang. For example, if your site is Chinese, you
  // may want to replace "en" with "zh-Hans".
  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
          routeBasePath: '/docs', // Docs at /docs
        },
        blog: false, // Disable blog
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    // Replace with your project's social card
    image: 'img/docusaurus-social-card.jpg',
    colorMode: {
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: 'Rover',
      logo: {
        alt: 'Rover Logo',
        src: 'img/rover-icon.svg',
      },
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'tutorialSidebar',
          position: 'left',
          label: 'Docs',
        },
        {
          href: 'https://github.com/thalesgelinger/rover',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Docs',
          items: [
            {
              label: 'Getting Started',
              to: '/docs/intro',
            },
          ],
        },
        {
          title: 'More',
          items: [
            {
              label: 'GitHub',
              href: 'https://github.com/thalesgelinger/rover',
            },
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} Rover. Built with Docusaurus.`,
    },
    prism: {
      theme: roverLightTheme,
      darkTheme: roverDarkTheme,
      additionalLanguages: ['lua', 'bash'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
