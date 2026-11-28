import { invoke } from "@tauri-apps/api/core";

import zhCN from "./zh-CN.json";
import en from "./en.json";
import { useSettingsStore } from "@/stores/settings";
import { isDesktop } from "@/utils/desktop";

function pushTrayLabels() {
  if (!isDesktop()) return;
  void invoke("clipboard_set_tray_labels", {
    labels: {
      pause: i18n.t("zdx.label.clipboard.pause"),
      resume: i18n.t("zdx.label.clipboard.resume"),
      enable: i18n.t("zdx.label.clipboard.enable"),
      ignoreNext: i18n.t("zdx.label.clipboard.ignoreNext"),
      ignoreNextArmed: i18n.t("zdx.label.clipboard.ignoreNextArmed"),
      screenshot: i18n.t("zdx.label.screenshot.tray"),
      recordScreen: i18n.t("zdx.label.screenshot.record"),
      recentShots: i18n.t("zdx.label.clipboard.quickShots"),
      hosts: i18n.t("zdx.label.clipboard.quickHosts"),
      openShots: i18n.t("zdx.label.screenshot.trayAll"),
      openHosts: i18n.t("zdx.label.ssh.trayAll"),
      noShots: i18n.t("zdx.text.screenshot.trayEmpty"),
      noHosts: i18n.t("zdx.text.ssh.emptyHosts"),
      docker: i18n.t("zdx.label.docker.title"),
      openDocker: i18n.t("zdx.label.docker.trayAll"),
      noEndpoints: i18n.t("zdx.text.docker.trayEmpty"),
      history: i18n.t("zdx.label.clipboard.openHistory"),
      settings: i18n.t("zdx.label.clipboard.settings"),
      quit: i18n.t("zdx.label.clipboard.quit"),
      show: i18n.t("zdx.label.clipboard.showPanel"),
      tooltipRecording: i18n.t("zdx.text.clipboard.trayRecording"),
      tooltipPaused: i18n.t("zdx.text.clipboard.trayPaused"),
      tooltipDisabled: i18n.t("zdx.text.clipboard.trayDisabled"),
      tooltipCapture: i18n.t("zdx.text.clipboard.trayCapture"),
      tooltipSsh: i18n.t("zdx.text.clipboard.traySsh"),
      tooltipDocker: i18n.t("zdx.text.clipboard.trayDocker"),
    },
  });
}

i18n.on("languageChanged", (language) => {
  document.documentElement.lang = language;
  pushTrayLabels();
});

void i18n.use(initReactI18next).init({
  resources: {
    "zh-CN": { translation: zhCN },
    en: { translation: en },
  },
  lng: useSettingsStore.getState().language,
  fallbackLng: "en",
  supportedLngs: ["zh-CN", "en"],
  interpolation: { escapeValue: false },
});

useSettingsStore.subscribe((state, previous) => {
  if (state.language !== previous.language) void i18n.changeLanguage(state.language);
});

export default i18n;
