import type { ThemeConfig } from "antd";
import type { ThemeMode } from "../stores/settings";

export const SHELL_BACKGROUND = { light: "#f3f4f6", dark: "#101012" } as const;

export function isDarkTheme(mode: ThemeMode, systemDark: boolean): boolean {
  return mode === "dark" || (mode === "system" && systemDark);
}

export function appTheme(dark: boolean): Omit<ThemeConfig, "algorithm"> {
  const layout = dark ? SHELL_BACKGROUND.dark : SHELL_BACKGROUND.light;
  return {
    token: {
      colorPrimary: "#4f5df0",
      colorInfo: "#4f5df0",
      borderRadius: 8,
      borderRadiusLG: 12,
      fontFamily: "-apple-system, BlinkMacSystemFont, \"Segoe UI\", \"PingFang SC\", \"Hiragino Sans GB\", \"Microsoft YaHei UI\", \"Microsoft YaHei\", \"Noto Sans CJK SC\", Ubuntu, Cantarell, sans-serif",
      colorBgLayout: layout,
      ...(dark ? { colorBgContainer: "#18181b", colorBgElevated: "#232327", colorBorderSecondary: "#2a2a2f", colorBorder: "#3a3a40" } : { colorBorderSecondary: "#ebedf0" }),
    },
    components: {
      Layout: { bodyBg: layout, siderBg: layout, headerBg: layout },
      Menu: {
        itemBg: "transparent",
        subMenuItemBg: "transparent",
        itemHeight: 36,
        itemMarginInline: 8,
        itemMarginBlock: 2,
        itemBorderRadius: 8,
        groupTitleFontSize: 12,
        activeBarBorderWidth: 0,
        ...(dark ? { itemSelectedBg: "rgba(255, 255, 255, 0.08)", itemSelectedColor: "#ffffff", itemHoverBg: "rgba(255, 255, 255, 0.05)" } : { itemSelectedBg: "#ffffff", itemSelectedColor: "#1f2329", itemHoverBg: "rgba(0, 0, 0, 0.04)" }),
      },
      Tabs: { horizontalMargin: "0 0 20px 0" },
      Alert: { withDescriptionPadding: "12px 16px" },
    },
  };
}
