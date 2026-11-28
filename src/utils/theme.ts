import type { CSSProperties } from "react";
import type { ConfigProviderProps, ThemeConfig } from "antd";

import type { ThemeMode } from "../stores/settings";

const FONT = "-apple-system, BlinkMacSystemFont, \"Segoe UI\", \"PingFang SC\", \"Hiragino Sans GB\", \"Microsoft YaHei UI\", \"Microsoft YaHei\", \"Noto Sans CJK SC\", Ubuntu, Cantarell, sans-serif";

export const ILLUSTRATION = {
  line: 1,
  radius: 8,
  radiusLG: 12,
  radiusSM: 6,
  shadow: 2,
  shadowSm: 1,
  controlHeight: 32,
  controlHeightSM: 28,
  controlHeightLG: 40,
} as const;

type Palette = {
  ink: string;
  text: string;
  muted: string;
  paper: string;
  container: string;
  elevated: string;
  sider: string;
  trigger: string;
  card: string;
  stroke: string;
  strokeSoft: string;
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

export const ACCENT = "#1677FF";

const CRAYON = {
  primary: ACCENT,
  success: "#51CF66",
  warning: "#FFD93D",
  error: "#FA5252",
  info: "#4DABF7",
} as const;

const LIGHT: Palette = {
  ink: "#241F1B",
  text: "#241F1B",
  muted: "#7A7268",
  paper: "#F3EEE6",
  container: "#FBF9F6",
  elevated: "#FFFFFF",
  sider: "#E7E0D4",
  trigger: "#DDD4C6",
  card: "#FFFFFF",
  stroke: "#D5CBBC",
  strokeSoft: "#E4DCD0",
  slider: "#0958D9",
  progressRemaining: "#BAE0FF",
  tooltipBg: "rgba(36, 31, 27, 0.92)",
  selectedBg: "#FFFFFF",
  selectedText: "#241F1B",
  hoverBg: "rgba(36, 31, 27, 0.06)",
  notifySuccess: "#F6FFED",
  notifyError: "#FFF1F0",
  notifyInfo: "#E6F4FF",
  notifyWarning: "#FFFBE6",
};

const DARK: Palette = {
  ink: "#EDE6DA",
  text: "#EDE6DA",
  muted: "#A79B8C",
  paper: "#100E0C",
  container: "#1C1916",
  elevated: "#26221E",
  sider: "#141210",
  trigger: "#2A2520",
  card: "#26221E",
  stroke: "#3F382F",
  strokeSoft: "#2E2924",
  slider: "#69B1FF",
  progressRemaining: "#15325B",
  tooltipBg: "rgba(20, 18, 16, 0.96)",
  selectedBg: "#312C27",
  selectedText: "#EDE6DA",
  hoverBg: "rgba(237, 230, 218, 0.08)",
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

export function inkFrameStyle(token: { lineWidth: number; colorBorder: string }, radius: number, offset: number = 0): CSSProperties {
  return {
    border: `${token.lineWidth}px solid ${token.colorBorder}`,
    borderRadius: radius,
    boxShadow: offset > 0 ? inkShadow(token.colorBorder, offset) : "none",
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
      colorTextSecondary: palette.muted,
      colorLink: ACCENT,
      colorBorder: palette.stroke,
      colorBorderSecondary: palette.strokeSoft,
      colorBgBase: palette.paper,
      colorBgLayout: palette.paper,
      colorBgContainer: palette.container,
      colorBgElevated: palette.elevated,
      colorTextLightSolid: "#FFFFFF",
      fontFamily: FONT,
      fontSize: 14,
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
        fontWeight: 500,
      },
      Modal: { boxShadow: "none" },
      Card: {
        boxShadow: "none",
        colorBgContainer: palette.card,
      },
      Tooltip: {
        colorBorder: palette.stroke,
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
        itemHeight: 36,
        itemMarginInline: 8,
        itemMarginBlock: 2,
        itemBorderRadius: ILLUSTRATION.radius,
        groupTitleFontSize: 12,
        groupTitleColor: palette.muted,
        itemColor: palette.text,
        itemHoverColor: palette.text,
        itemHoverBg: palette.hoverBg,
        itemSelectedBg: palette.selectedBg,
        itemSelectedColor: palette.selectedText,
        darkItemBg: "transparent",
        darkSubMenuItemBg: "transparent",
        darkGroupTitleColor: palette.muted,
        darkItemColor: palette.text,
        darkItemHoverColor: palette.text,
        darkItemHoverBg: palette.hoverBg,
        darkItemSelectedBg: palette.selectedBg,
        darkItemSelectedColor: palette.selectedText,
      },
      Tabs: { horizontalMargin: "0 0 16px 0" },
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
    segmented: { classNames: { root: "pixo-ink-segmented" } },
    progress: {
      classNames: { rail: "pixo-progress-rail", track: "pixo-progress-track" },
      styles: { rail: { height: 16 }, track: { height: 10 } },
    },
  };
}
