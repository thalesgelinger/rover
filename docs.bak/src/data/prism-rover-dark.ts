import type { PrismTheme } from 'prism-react-renderer';

/**
 * Rover Dark Theme - Soft pastry colors with distinct background
 * Brand colors: Lighter Carmesin #ff3b79, Bright Red #ff2e71, Ice #f2f2f2
 */
export const roverDarkTheme: PrismTheme = {
  plain: {
    color: '#d1d5db',
    backgroundColor: '#0f0f0f',
  },
  styles: [
    {
      types: ['comment', 'prolog', 'doctype', 'cdata'],
      style: {
        color: '#6b7280',
        fontStyle: 'italic',
      },
    },
    {
      types: ['namespace'],
      style: {
        opacity: 0.7,
      },
    },
    {
      types: ['string', 'attr-value'],
      style: {
        color: '#fca5a5',
      },
    },
    {
      types: ['punctuation', 'operator'],
      style: {
        color: '#9ca3af',
      },
    },
    {
      types: ['entity', 'url', 'symbol', 'number', 'boolean', 'variable', 'constant', 'property', 'regex', 'inserted'],
      style: {
        color: '#fda4af',
      },
    },
    {
      types: ['atrule', 'keyword', 'attr-name', 'selector'],
      style: {
        color: '#f9a8d4',
      },
    },
    {
      types: ['function', 'deleted', 'tag'],
      style: {
        color: '#fdba74',
      },
    },
    {
      types: ['function-variable'],
      style: {
        color: '#d8b4fe',
      },
    },
    {
      types: ['tag', 'selector', 'keyword'],
      style: {
        color: '#f9a8d4',
      },
    },
  ],
};
