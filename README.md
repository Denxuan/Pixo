# Tauri + React + Typescript

This template should help get you started developing with Tauri, React and Typescript in Vite.

## UI 组件

项目使用 Ant Design 和 UnoCSS。前端框架 API、组件及图标按使用需要配置自动导入，Tauri API 显式导入，项目组件通过 `@/` 显式导入。用户可见文案按 `AGENTS.md` 中的 i18n 规则管理。

路由使用 React Router，共享状态使用 Zustand；常用路由 API 和 `createZustandStore` 已配置自动导入。桌面端路由可使用 `HashRouter`；具体路由和状态随功能创建。

国际化使用 i18next 和 react-i18next，资源位于 `src/lang`。默认简体中文、回退英文；在组件中使用自动导入的 `useTranslation()` 获取 `t`，通过 `useSettingsStore` 切换语言。主题和语言偏好由该仓库统一保存。

## 环境配置

根目录的 `.env` 中通过 `VITE_APP_TITLE` 设置页面标题；修改后需重启开发服务器。`VITE_` 开头的变量会包含在前端产物中，不要用于保存密钥。Tauri 窗口标题在 `src-tauri/tauri.conf.json` 中单独配置。

## 发行与 Homebrew

将 `package.json`、`src-tauri/tauri.conf.json` 和 `src-tauri/Cargo.toml` 中的版本统一后，推送对应的稳定版本标签（例如 `v0.1.0`）。GitHub Actions 会构建 macOS 通用版、Windows 安装包和 Linux 安装包，创建 GitHub Release，并将包含实际 SHA-256 的 Homebrew Cask 写入默认分支。仓库的 Actions 需要有 `contents: write` 权限；受保护分支需允许该更新。发行后请先同步默认分支再进行本地开发。

首次发行成功后，可通过以下命令安装 macOS 版本（将仓库地址改为实际公开的 GitHub 地址）：

```sh
brew tap denxuan/tab
brew install --cask pixo
```

macOS 发行包使用临时签名，尚未经过 Apple 公证；首次打开时 macOS 可能要求在系统设置中确认。该配置适合测试分发，面向正式用户发布时应配置 Developer ID 签名和公证。

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
