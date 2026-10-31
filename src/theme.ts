// 主题注册表：每套主题 = 界面 CSS 变量 + 终端配色，二者联动切换。
// 切换入口：applyTheme(id)（写 CSS 变量到 :root）；持久化在设置 ui_theme。

export interface TermPalette {
  background: string;
  foreground: string;
  cursor: string;
  cursorAccent: string;
  selectionBackground: string;
  black: string;
  red: string;
  green: string;
  yellow: string;
  blue: string;
  magenta: string;
  cyan: string;
  white: string;
  brightBlack: string;
  brightRed: string;
  brightGreen: string;
  brightYellow: string;
  brightBlue: string;
  brightMagenta: string;
  brightCyan: string;
  brightWhite: string;
}

export interface ThemeDef {
  id: string;
  name: string;
  dark: boolean;
  vars: {
    bg: string;
    panel: string;
    border: string;
    hover: string;
    text: string;
    dim: string;
    faint: string;
    accent: string;
    accentSoft: string;
    ok: string;
    wsBg: string;
    wsChrome: string;
    wsBorder: string;
    wsBtn: string;
    wsBtnHover: string;
    wsText: string;
    wsTextDim: string;
  };
  term: TermPalette;
}

const ansi = (
  black: string, red: string, green: string, yellow: string,
  blue: string, magenta: string, cyan: string, white: string,
  bBlack: string, bRed: string, bGreen: string, bYellow: string,
  bBlue: string, bMagenta: string, bCyan: string, bWhite: string,
): Pick<
  TermPalette,
  | "black" | "red" | "green" | "yellow" | "blue" | "magenta" | "cyan" | "white"
  | "brightBlack" | "brightRed" | "brightGreen" | "brightYellow"
  | "brightBlue" | "brightMagenta" | "brightCyan" | "brightWhite"
> => ({
  black, red, green, yellow, blue, magenta, cyan, white,
  brightBlack: bBlack, brightRed: bRed, brightGreen: bGreen,
  brightYellow: bYellow, brightBlue: bBlue, brightMagenta: bMagenta,
  brightCyan: bCyan, brightWhite: bWhite,
});

export const THEMES: ThemeDef[] = [
  {
    id: "midnight",
    name: "深空",
    dark: true,
    vars: {
      bg: "#0e1525", panel: "#141e30", border: "#223048", hover: "#1b2740",
      text: "#dce4f2", dim: "#8b9bb8", faint: "#5c6e8c",
      accent: "#4c8dff", accentSoft: "#16304f", ok: "#3fd68f",
      wsBg: "#0b1120", wsChrome: "#101a2b", wsBorder: "#223048",
      wsBtn: "#1a2740", wsBtnHover: "#243352",
      wsText: "#dce4f2", wsTextDim: "#8b9bb8",
    },
    term: {
      background: "#0e1525", foreground: "#dce4f2",
      cursor: "#4c8dff", cursorAccent: "#0e1525", selectionBackground: "#29406b",
      ...ansi(
        "#1a2740", "#ff6b6b", "#5fdca0", "#ffd166",
        "#6cb2ff", "#c792ea", "#6ae2f0", "#dce4f2",
        "#44598c", "#ff9b9b", "#8fe8c0", "#ffe3a3",
        "#9cc9ff", "#e0b7ff", "#9ceef5", "#ffffff",
      ),
    },
  },
  {
    id: "daylight",
    name: "晨光",
    dark: false,
    vars: {
      bg: "#f5f6f8", panel: "#ffffff", border: "#e2e5ea", hover: "#f0f2f5",
      text: "#1a1d21", dim: "#6b7280", faint: "#9aa1ab",
      accent: "#2f6fed", accentSoft: "#e8f0fe", ok: "#0a8a4a",
      wsBg: "#eef0f4", wsChrome: "#ffffff", wsBorder: "#dfe3e8",
      wsBtn: "#ffffff", wsBtnHover: "#e8ebf0",
      wsText: "#1a1d21", wsTextDim: "#6b7280",
    },
    term: {
      background: "#ffffff", foreground: "#333333",
      cursor: "#2f6fed", cursorAccent: "#ffffff", selectionBackground: "#cfe0fb",
      ...ansi(
        "#333333", "#d13d3d", "#2a8a3a", "#b58900",
        "#268bd2", "#d33682", "#2aa198", "#eeeeee",
        "#555555", "#e06c75", "#98c379", "#d19a66",
        "#61afef", "#c678dd", "#56b6c2", "#ffffff",
      ),
    },
  },
  {
    id: "geekblack",
    name: "极客黑",
    dark: true,
    vars: {
      bg: "#0d1117", panel: "#161b22", border: "#30363d", hover: "#21262d",
      text: "#e6edf3", dim: "#8b949e", faint: "#6e7681",
      accent: "#2f81f7", accentSoft: "#121d2f", ok: "#3fb950",
      wsBg: "#0d1117", wsChrome: "#161b22", wsBorder: "#30363d",
      wsBtn: "#21262d", wsBtnHover: "#30363d",
      wsText: "#e6edf3", wsTextDim: "#8b949e",
    },
    term: {
      background: "#0d1117", foreground: "#e6edf3",
      cursor: "#2f81f7", cursorAccent: "#0d1117", selectionBackground: "#264f78",
      ...ansi(
        "#484f58", "#ff7b72", "#3fb950", "#d29922",
        "#58a6ff", "#bc8cff", "#39c5cf", "#b1bac4",
        "#6e7681", "#ffa198", "#56d364", "#e3b341",
        "#79c0ff", "#d2a8ff", "#56d4dd", "#f0f6fc",
      ),
    },
  },
  {
    id: "dracula",
    name: "Dracula",
    dark: true,
    vars: {
      bg: "#21222c", panel: "#282a36", border: "#44475a", hover: "#343746",
      text: "#f8f8f2", dim: "#a3a7c2", faint: "#6272a4",
      accent: "#bd93f9", accentSoft: "#3b3b52", ok: "#50fa7b",
      wsBg: "#21222c", wsChrome: "#282a36", wsBorder: "#44475a",
      wsBtn: "#343746", wsBtnHover: "#44475a",
      wsText: "#f8f8f2", wsTextDim: "#a3a7c2",
    },
    term: {
      background: "#282a36", foreground: "#f8f8f2",
      cursor: "#f8f8f2", cursorAccent: "#282a36", selectionBackground: "#44475a",
      ...ansi(
        "#21222c", "#ff5555", "#50fa7b", "#f1fa8c",
        "#bd93f9", "#ff79c6", "#8be9fd", "#f8f8f2",
        "#6272a4", "#ff6e6e", "#69ff94", "#ffffa5",
        "#d6acff", "#ff92df", "#a4ffff", "#ffffff",
      ),
    },
  },
  {
    id: "kanagawa",
    name: "Kanagawa Wave",
    dark: true,
    vars: {
      bg: "#1f1f28", panel: "#262633", border: "#363646", hover: "#2a2a37",
      text: "#dcd7ba", dim: "#a9a79a", faint: "#727169",
      accent: "#7e9cd8", accentSoft: "#2d425a", ok: "#76946a",
      wsBg: "#16161d", wsChrome: "#1f1f28", wsBorder: "#363646",
      wsBtn: "#2a2a37", wsBtnHover: "#363646",
      wsText: "#dcd7ba", wsTextDim: "#a9a79a",
    },
    term: {
      background: "#1f1f28", foreground: "#dcd7ba",
      cursor: "#c8c093", cursorAccent: "#1f1f28", selectionBackground: "#2d4f67",
      ...ansi(
        "#090618", "#c34043", "#76946a", "#c0a36e",
        "#7e9cd8", "#957fb8", "#6a9589", "#c8c093",
        "#727169", "#e82424", "#98bb6c", "#ffa066",
        "#7fb4ca", "#938fa9", "#7aa89f", "#dcd7ba",
      ),
    },
  },
  {
    id: "nord",
    name: "Nord",
    dark: true,
    vars: {
      bg: "#2e3440", panel: "#3b4252", border: "#4c566a", hover: "#434c5e",
      text: "#eceff4", dim: "#b8c1cf", faint: "#7b88a1",
      accent: "#88c0d0", accentSoft: "#3b4252", ok: "#a3be8c",
      wsBg: "#242933", wsChrome: "#2e3440", wsBorder: "#4c566a",
      wsBtn: "#3b4252", wsBtnHover: "#434c5e",
      wsText: "#eceff4", wsTextDim: "#b8c1cf",
    },
    term: {
      background: "#2e3440", foreground: "#eceff4",
      cursor: "#d8dee9", cursorAccent: "#2e3440", selectionBackground: "#434c5e",
      ...ansi(
        "#3b4252", "#bf616a", "#a3be8c", "#ebcb8b",
        "#81a1c1", "#b48ead", "#88c0d0", "#e5e9f0",
        "#4c566a", "#bf616a", "#a3be8c", "#ebcb8b",
        "#81a1c1", "#b48ead", "#8fbcbb", "#eceff4",
      ),
    },
  },
  {
    id: "solarized-light",
    name: "Solarized Light",
    dark: false,
    vars: {
      bg: "#fdf6e3", panel: "#fffbef", border: "#e3ddc8", hover: "#eee8d5",
      text: "#586e75", dim: "#4d6570", faint: "#7b8784",
      accent: "#268bd2", accentSoft: "#eee8d5", ok: "#859900",
      wsBg: "#f3ecd8", wsChrome: "#fdf6e3", wsBorder: "#e3ddc8",
      wsBtn: "#fffbef", wsBtnHover: "#eee8d5",
      wsText: "#586e75", wsTextDim: "#4d6570",
    },
    term: {
      background: "#fdf6e3", foreground: "#657b83",
      cursor: "#657b83", cursorAccent: "#fdf6e3", selectionBackground: "#eee8d5",
      ...ansi(
        "#073642", "#dc322f", "#859900", "#b58900",
        "#268bd2", "#d33682", "#2aa198", "#eee8d5",
        "#002b36", "#cb4b16", "#586e75", "#657b83",
        "#839496", "#6c71c4", "#93a1a1", "#fdf6e3",
      ),
    },
  },
];

export const DEFAULT_THEME_ID = "midnight";

/** 主机名 → 稳定色相（卡片图标、连接加载卡共用） */
export function hostHue(name: string): number {
  let h = 0;
  for (let i = 0; i < name.length; i += 1) {
    h = (h * 31 + name.charCodeAt(i)) % 360;
  }
  return h;
}

export function getTheme(id: string | null | undefined): ThemeDef {
  return THEMES.find((t) => t.id === id) ?? THEMES.find((t) => t.id === DEFAULT_THEME_ID)!;
}

/** 把主题变量写到 :root，界面全部经 var() 消费 */
export function applyTheme(id: string | null | undefined): ThemeDef {
  const theme = getTheme(id);
  const root = document.documentElement;
  for (const [key, value] of Object.entries(theme.vars)) {
    root.style.setProperty(
      `--${key.replace(/[A-Z]/g, (m) => "-" + m.toLowerCase())}`,
      value,
    );
  }
  root.dataset.theme = theme.id;
  void import("./lib/ipc").then((m) => m.setWindowTheme(theme.dark));
  return theme;
}
