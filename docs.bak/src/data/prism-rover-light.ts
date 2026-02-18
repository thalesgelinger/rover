import type { PrismTheme } from 'prism-react-renderer';

/**
 * Rover Light Theme - Soft pastry colors with distinct background
 * Brand colors: Carmesin #FF155F, Bright Red #D40000, Dark Gray #1a1a1a
 */
export const roverLightTheme: PrismTheme = {
  plain: {
    color: '#2d2d2d',
    backgroundColor: '#fafafa',
  },
  styles: [
    {
      types: ['comment', 'prolog', 'doctype', 'cdata'],
      style: {
        color: '#9ca3af',
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
        color: '#e05d5d',
      },
    },
    {
      types: ['punctuation', 'operator'],
      style: {
        color: '#6b7280',
      },
    },
    {
      types: ['entity', 'url', 'symbol', 'number', 'boolean', 'variable', 'constant', 'property', 'regex', 'inserted'],
      style: {
        color: '#ff6b9d',
      },
    },
    {
      types: ['atrule', 'keyword', 'attr-name', 'selector'],
      style: {
        color: '#d87a7a',
      },
    },
    {
      types: ['function', 'deleted', 'tag'],
      style: {
        color: '#ff7aa6',
      },
    },
    {
      types: ['function-variable'],
      style: {
        color: '#9d7dbd',
      },
    },
    {
      types: ['tag', 'selector', 'keyword'],
      style: {
        color: '#d87a7a',
      },
    },
  ],
};
