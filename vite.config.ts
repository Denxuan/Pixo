import { defineConfig } from "vite";
import react, { reactCompilerPreset } from "@vitejs/plugin-react";
import babel from "@rolldown/plugin-babel";
import AutoImport from "unplugin-auto-import/vite";
import UnoCSS from "unocss/vite";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [
    AutoImport({
      imports: [
        "react",
        {
          react: ["StrictMode"],
          "react-dom/client": ["createRoot"],
          "react-i18next": ["initReactI18next", "useTranslation", "Trans"],
          i18next: [["default", "i18n"]],
          "react-router": ["HashRouter", "Routes", "Route", "Link", "NavLink", "Outlet", "Navigate", "useNavigate", "useLocation", "useParams"],
          zustand: [["create", "createZustandStore"]],
          "zustand/middleware": ["persist"],
          antd: ["Button", "ConfigProvider", "Input", "Modal", "Switch", ["theme", "antdTheme"]],
          "@ant-design/icons": [
            "SendOutlined",
            "AppstoreOutlined",
            "GlobalOutlined",
            "CodeOutlined",
            "HistoryOutlined",
            "FontSizeOutlined",
            "FileImageOutlined",
            "FileOutlined",
            "PushpinOutlined",
            "CopyOutlined",
            "DeleteOutlined",
            "PauseOutlined",
            "SettingOutlined",
            "SearchOutlined",
            "PlayCircleOutlined",
          ],
        },
      ],
      dts: "src/types/auto-imports.d.ts",
      dtsMode: "overwrite",
    }),
    UnoCSS(),
    react(),
    babel({ presets: [reactCompilerPreset()] }),
  ],
  resolve: { tsconfigPaths: true },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
