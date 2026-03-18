import serverSnippet from './server-snippet';
import websocketSnippet from './websocket-snippet';
import uiSnippet from './ui-snippet';

export const snippets = [
  {
    label: "Server",
    value: "server",
    code: serverSnippet,
    wip: false,
  },
  {
    label: "WebSocket",
    value: "websocket", 
    code: websocketSnippet,
    wip: true,
  },
  {
    label: "UI",
    value: "ui",
    code: uiSnippet,
    wip: true,
  },
];
