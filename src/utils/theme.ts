import type { CSSProperties } from "react";
import type { ConfigProviderProps, ThemeConfig } from "antd";

import type { ThemeMode } from "../stores/settings";

const FONT = "-apple-system, BlinkMacSystemFont, \"Segoe UI\", \"PingFang SC\", \"Hiragino Sans GB\", \"Microsoft YaHei UI\", \"Microsoft YaHei\", \"Noto Sans CJK SC\", Ubuntu, Cantarell, sans-serif";

export const ILLUSTRATION = {
  line: 3,
  radius: 12,
  radiusLG: 16,
  radiusSM: 8,
  shadow: 4,
  shadowSm: 2,
  controlHeight: 40,
  controlHeightSM: 34,
  controlHeightLG: 48,
} as const;

type Palette = {
  ink: string;
  text: string;
  paper: string;
  container: string;
  elevated: string;
  sider: string;
  trigger: string;
  card: string;
  slider: string;
  progressRemaining: string;
  tooltipBg: string;
  selectedBg: string;
  selectedText: string;
  hoverBg: string;
  notifySuccess: string;
  notifyError: string;
  notifyInfo: string;
  notifyWarning: string;
};

const CRAYON = {
  primary: "#52C41A",
  success: "#51CF66",
  warning: "#FFD93D",
  error: "#FA5252",
  info: "#4DABF7",
} as const;

const LIGHT: Palette = {
  ink: "#2C2C2C",
  text: "#2C2C2C",
  paper: "#FFF9F0",
  container: "#FFFFFF",
  elevated: "#FFFFFF",
  sider: "#FFF0F6",
  trigger: "#FFE7BA",
  card: "#FFF0F6",
  slider: "#237804",
  progressRemaining: "#D9F7BE",
  tooltipBg: "rgba(100, 100, 100, 0.95)",
  selectedBg: "#FFFFFF",
  selectedText: "#2C2C2C",
  hoverBg: "rgba(44, 44, 44, 0.06)",
  notifySuccess: "#F6FFED",
  notifyError: "#FFF1F0",
  notifyInfo: "#E6F4FF",
  notifyWarning: "#FFFBE6",
};

const DARK: Palette = {
  ink: "#F4E7D4",
  text: "#F4E7D4",
  paper: "#1C1612",
  container: "#2A221C",
  elevated: "#342820",
  sider: "#3A2832",
  trigger: "#4A3424",
  card: "#3A2832",
  slider: "#95DE64",
  progressRemaining: "#274916",
  tooltipBg: "rgba(42, 34, 28, 0.96)",
  selectedBg: "#F4E7D4",
  selectedText: "#2C2C2C",
  hoverBg: "rgba(244, 231, 212, 0.12)",
  notifySuccess: "#162312",
  notifyError: "#2A1215",
  notifyInfo: "#111A2C",
  notifyWarning: "#2B2111",
};

export const SHELL_BACKGROUND = { light: LIGHT.paper, dark: DARK.paper } as const;

export function isDarkTheme(mode: ThemeMode, systemDark: boolean): boolean {
  return mode === "dark" || (mode === "system" && systemDark);
}

export function illustrationPalette(dark: boolean): Palette {
  return dark ? DARK : LIGHT;
}

export function inkShadow(color: string, offset: number = ILLUSTRATION.shadow): string {
  return `${offset}px ${offset}px 0 ${color}`;
}

export function inkFrameStyle(token: { lineWidth: number; colorBorder: string }, radius: number, offset: number = ILLUSTRATION.shadow): CSSProperties {
  return {
    border: `${token.lineWidth}px solid ${token.colorBorder}`,
    borderRadius: radius,
    boxShadow: inkShadow(token.colorBorder, offset),
  };
}

export function appTheme(dark: boolean): Omit<ThemeConfig, "algorithm"> {
  const palette = illustrationPalette(dark);
  return {
    token: {
      colorPrimary: CRAYON.primary,
      colorSuccess: CRAYON.success,
      colorWarning: CRAYON.warning,
      colorError: CRAYON.error,
      colorInfo: CRAYON.info,
      colorTextBase: palette.text,
      colorText: palette.text,
      colorBorder: palette.ink,
      colorBorderSecondary: palette.ink,
      colorBgBase: palette.paper,
      colorBgLayout: palette.paper,
      colorBgContainer: palette.container,
      colorBgElevated: palette.elevated,
      colorTextLightSolid: "#FFFFFF",
      fontFamily: FONT,
      fontSize: 15,
      fontWeightStrong: 600,
      lineWidth: ILLUSTRATION.line,
      lineWidthBold: ILLUSTRATION.line,
      borderRadius: ILLUSTRATION.radius,
      borderRadiusLG: ILLUSTRATION.radiusLG,
      borderRadiusSM: ILLUSTRATION.radiusSM,
      controlHeight: ILLUSTRATION.controlHeight,
      controlHeightSM: ILLUSTRATION.controlHeightSM,
      controlHeightLG: ILLUSTRATION.controlHeightLG,
    },
    components: {
      Button: {
        primaryShadow: "none",
        dangerShadow: "none",
        defaultShadow: "none",
        fontWeight: 600,
      },
      Modal: { boxShadow: "none" },
      Card: {
        boxShadow: inkShadow(palette.ink),
        colorBgContainer: palette.card,
      },
      Tooltip: {
        colorBorder: palette.ink,
        colorBgSpotlight: palette.tooltipBg,
        borderRadius: ILLUSTRATION.radiusSM,
      },
      Select: { optionSelectedBg: "transparent" },
      Slider: {
        dotBorderColor: palette.slider,
        dotActiveBorderColor: palette.slider,
        colorPrimaryBorder: palette.slider,
        colorPrimaryBorderHover: palette.slider,
      },
      Notification: {
        colorSuccessBg: palette.notifySuccess,
        colorErrorBg: palette.notifyError,
        colorInfoBg: palette.notifyInfo,
        colorWarningBg: palette.notifyWarning,
      },
      Layout: {
        bodyBg: palette.paper,
        footerBg: palette.paper,
        headerBg: palette.container,
        headerColor: palette.text,
        siderBg: palette.sider,
        lightSiderBg: palette.sider,
        triggerBg: palette.trigger,
        triggerColor: palette.text,
        lightTriggerBg: palette.trigger,
        lightTriggerColor: palette.text,
      },
      Menu: {
        activeBarBorderWidth: 0,
        itemBg: "transparent",
        subMenuItemBg: "transparent",
        itemHeight: 40,
        itemMarginInline: 8,
        itemMarginBlock: 4,
        itemBorderRadius: ILLUSTRATION.radius,
        groupTitleFontSize: 12,
        groupTitleColor: palette.text,
        itemColor: palette.text,
        itemHoverColor: palette.text,
        itemHoverBg: palette.hoverBg,
        itemSelectedBg: palette.selectedBg,
        itemSelectedColor: palette.selectedText,
        darkItemBg: "transparent",
        darkSubMenuItemBg: "transparent",
        darkGroupTitleColor: palette.text,
        darkItemColor: palette.text,
        darkItemHoverColor: palette.text,
        darkItemHoverBg: palette.hoverBg,
        darkItemSelectedBg: palette.selectedBg,
        darkItemSelectedColor: palette.selectedText,
      },
      Tabs: { horizontalMargin: "0 0 20px 0" },
      Alert: { withDescriptionPadding: "12px 16px" },
      Progress: {
        circleTextColor: palette.text,
        defaultColor: CRAYON.primary,
        remainingColor: palette.progressRemaining,
      },
    },
  };
}

const field = { classNames: { root: "pixo-ink-field" } };
const popup = { classNames: { root: "pixo-ink-popup" } };

export function illustrationConfig(dark: boolean): Omit<ConfigProviderProps, "children"> {
  return {
    theme: appTheme(dark),
    button: {
      autoInsertSpace: false,
      classNames: { root: "pixo-ink-button" },
    },
    modal: { classNames: { container: "pixo-ink-box" } },
    drawer: { classNames: { section: "pixo-ink-box" } },
    alert: { className: "pixo-ink-border" },
    colorPicker: { arrow: false, classNames: { root: "pixo-ink-box" } },
    popover: { classNames: { container: "pixo-ink-box" } },
    popconfirm: { classNames: { container: "pixo-ink-box" } },
    tooltip: {
      arrow: false,
      classNames: { root: "pixo-ink-tooltip", container: "pixo-ink-box" },
    },
    notification: {
      classNames: { root: "pixo-ink-notice", title: "pixo-ink-title" },
    },
    dropdown: popup,
    select: {
      classNames: {
        root: "pixo-ink-field",
        popup: { root: "pixo-ink-popup" },
      },
    },
    input: field,
    inputPassword: field,
    textArea: field,
    inputNumber: {
      classNames: { root: "pixo-ink-field", actions: "pixo-ink-actions" },
    },
    segmented: { classNames: { root: "pixo-ink-box" } },
    progress: {
      classNames: { rail: "pixo-progress-rail", track: "pixo-progress-track" },
      styles: { rail: { height: 16 }, track: { height: 10 } },
    },
  };
}
