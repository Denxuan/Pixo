# Tauri + React + Typescript

This template should help get you started developing with Tauri, React and Typescript in Vite.

## UI 组件

项目使用 Ant Design 和 UnoCSS。前端框架 API、组件及图标按使用需要配置自动导入，Tauri API 显式导入，项目组件通过 `@/` 显式导入。用户可见文案按 `AGENTS.md` 中的 i18n 规则管理。

路由使用 React Router，共享状态使用 Zustand；常用路由 API 和 `createZustandStore` 已配置自动导入。桌面端路由可使用 `HashRouter`；具体路由和状态随功能创建。

国际化使用 i18next 和 react-i18next，资源位于 `src/lang`。默认简体中文、回退英文；在组件中使用自动导入的 `useTranslation()` 获取 `t`，通过 `useSettingsStore` 切换语言。主题和语言偏好由该仓库统一保存。

## 环境配置

根目录的 `.env` 中通过 `VITE_APP_TITLE` 设置页面标题；修改后需重启开发服务器。`VITE_` 开头的变量会包含在前端产物中，不要用于保存密钥。Tauri 窗口标题在 `src-tauri/tauri.conf.json` 中单独配置。

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
