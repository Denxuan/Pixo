import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { ClipboardRequestError, clipboardClear, clipboardGetSettings } from "@/api/clipboard";
import { dataProfile, exportBundle, type DataProfile } from "@/api/system";
import { SettingGroup, SettingRow } from "@/components/SettingGroup";
import type { ClipboardSettings } from "@/types/clipboard";
import { clipboardErrorText } from "@/utils/clipboardMessage";
import { isDesktop } from "@/utils/desktop";

export default function DataSettingsPage() {
  const { t } = useTranslation();
  const desktop = isDesktop();
  const [profile, setProfile] = useState<DataProfile | null>(null);
  const [settings, setSettings] = useState<ClipboardSettings | null>(null);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState<"hosts" | "endpoints" | "">("");

  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    void Promise.all([dataProfile(), clipboardGetSettings()])
      .then(([nextProfile, nextSettings]) => {
        if (!alive) return;
        setProfile(nextProfile);
        setSettings(nextSettings);
      })
      .catch(() => {
        if (alive) setError(t("zdx.text.system.dataFailed"));
      });
    return () => {
      alive = false;
    };
  }, [desktop, t]);

  async function reveal(path: string) {
    setNotice("");
    setError("");
    try {
      await revealItemInDir(path);
    } catch {
      setError(t("zdx.text.system.dataFailed"));
    }
  }

  async function exportKind(kind: "hosts" | "endpoints") {
    setBusy(kind);
    setNotice("");
    setError("");
    try {
      const saved = await exportBundle(kind, t(kind === "hosts" ? "zdx.label.system.exportHosts" : "zdx.label.system.exportEndpoints"));
      if (saved) setNotice(t("zdx.text.system.exportDone"));
    } catch {
      setError(t("zdx.text.system.dataFailed"));
    } finally {
      setBusy("");
    }
  }

  function clearClipboard() {
    Modal.confirm({
      title: t("zdx.text.system.clearClipboardTitle"),
      content: t("zdx.text.system.clearClipboardBody"),
      okText: t("zdx.label.system.clearClipboard"),
      cancelText: t("zdx.label.clipboard.cancel"),
      okButtonProps: { danger: true },
      onOk: async () => {
        try {
          await clipboardClear(true);
          setNotice(t("zdx.text.system.clearClipboardDone"));
        } catch (caught) {
          setError(caught instanceof ClipboardRequestError ? clipboardErrorText(t, caught) : t("zdx.text.system.dataFailed"));
        }
      },
    });
  }

  const retention = settings
    ? settings.retentionDays > 0
      ? t("zdx.text.system.dataClipboard", { count: settings.maxItems, days: settings.retentionDays })
      : t("zdx.text.system.dataClipboardKeep", { count: settings.maxItems })
    : t("zdx.text.system.sessionPending");
  const activity = [
    profile?.recording ? t("zdx.text.system.activityRecording") : "",
    profile && profile.sshSessions > 0 ? t("zdx.text.system.activitySsh", { count: profile.sshSessions }) : "",
    profile && profile.dockerLinks > 0 ? t("zdx.text.system.activityDocker", { count: profile.dockerLinks }) : "",
  ].filter(Boolean);

  return (
    <Flex vertical gap={24}>
      {!desktop && <Alert type="info" showIcon message={t("zdx.text.system.desktopOnly")} />}
      {error && <Alert type="error" showIcon message={error} />}
      {notice && <Alert type="success" showIcon message={notice} />}
      {activity.length > 0 && <Alert type="info" showIcon message={activity.join(t("zdx.text.system.activityJoin"))} />}
      <SettingGroup title={t("zdx.label.system.dataPlace")}>
        <SettingRow label={t("zdx.label.system.dataClipboard")} description={profile?.clipboardDb || retention}>
          {profile && <Button onClick={() => void reveal(profile.clipboardDb)}>{t("zdx.label.system.reveal")}</Button>}
        </SettingRow>
        <SettingRow label={t("zdx.label.system.dataShots")} description={profile?.screenshotLibrary}>
          {profile && <Button onClick={() => void reveal(profile.screenshotLibrary)}>{t("zdx.label.system.reveal")}</Button>}
        </SettingRow>
        <SettingRow label={t("zdx.label.system.dataShotFiles")} description={profile?.screenshotSaveDir}>
          {profile?.screenshotSaveDir && <Button onClick={() => void reveal(profile.screenshotSaveDir)}>{t("zdx.label.system.reveal")}</Button>}
        </SettingRow>
        <SettingRow label={t("zdx.label.system.dataSsh")} description={profile?.sshDb}>
          {profile && <Button onClick={() => void reveal(profile.sshDb)}>{t("zdx.label.system.reveal")}</Button>}
        </SettingRow>
        <SettingRow label={t("zdx.label.system.dataDocker")} description={profile?.dockerDb}>
          {profile && <Button onClick={() => void reveal(profile.dockerDb)}>{t("zdx.label.system.reveal")}</Button>}
        </SettingRow>
      </SettingGroup>
      <SettingGroup title={t("zdx.label.system.dataLife")}>
        <SettingRow label={t("zdx.label.system.dataClipboard")} description={retention}>
          <Button danger onClick={clearClipboard} disabled={!desktop}>{t("zdx.label.system.clearClipboard")}</Button>
        </SettingRow>
        <SettingRow label={t("zdx.label.system.dataImages")} description={t("zdx.text.system.dataImages")} />
        <SettingRow label={t("zdx.label.system.dataEncryption")} description={t("zdx.text.system.dataEncryption")} />
      </SettingGroup>
      <SettingGroup title={t("zdx.label.system.dataSession")}>
        <SettingRow label={t("zdx.label.system.sessionHide")} description={t("zdx.text.system.sessionHide")} />
        <SettingRow label={t("zdx.label.system.sessionQuit")} description={t("zdx.text.system.sessionQuit")} />
        <SettingRow label={t("zdx.label.system.sessionRestart")} description={t("zdx.text.system.sessionRestart")} />
      </SettingGroup>
      <SettingGroup title={t("zdx.label.system.dataExport")}>
        <SettingRow label={t("zdx.label.system.exportHosts")} description={t("zdx.text.system.exportHosts")}>
          <Button onClick={() => void exportKind("hosts")} loading={busy === "hosts"} disabled={!desktop}>{t("zdx.label.system.exportHosts")}</Button>
        </SettingRow>
        <SettingRow label={t("zdx.label.system.exportEndpoints")} description={t("zdx.text.system.exportEndpoints")}>
          <Button onClick={() => void exportKind("endpoints")} loading={busy === "endpoints"} disabled={!desktop}>{t("zdx.label.system.exportEndpoints")}</Button>
        </SettingRow>
      </SettingGroup>
    </Flex>
  );
}
