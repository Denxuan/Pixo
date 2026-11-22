import { ClipboardRequestError, clipboardGetSettings, clipboardStatus, clipboardUpdateSettings, toClipboardError } from "@/api/clipboard";
import ExcludedAppsField from "@/components/clipboard/ExcludedAppsField";
import ShortcutField from "@/components/clipboard/ShortcutField";
import { SettingGroup, SettingRow } from "@/components/SettingGroup";
import { useAutosave } from "@/hooks/useAutosave";
import type { ClipboardSettings, ClipboardStatus } from "@/types/clipboard";
import { toSettingsUpdate } from "@/types/clipboard";
import { clipboardErrorText } from "@/utils/clipboardMessage";
import { isDesktop } from "@/utils/desktop";
import { DEFAULT_DELETE_SHORTCUT, DEFAULT_PIN_SHORTCUT } from "@/utils/shortcut";
import { bytesToMegabytes, megabytesToBytes } from "@/utils/time";

export default function ClipboardSettingsPage() {
  const { t } = useTranslation();
  const desktop = isDesktop();
  const [draft, setDraft] = useState<ClipboardSettings | null>(null);
  const [status, setStatus] = useState<ClipboardStatus | null>(null);
  const [textMb, setTextMb] = useState<number | null>(1);
  const [imageMb, setImageMb] = useState<number | null>(20);
  const [excluded, setExcluded] = useState("");
  const [excludedApps, setExcludedApps] = useState<string[]>([]);
  const [ignored, setIgnored] = useState("");
  const [error, setError] = useState("");
  const [confirmCount, setConfirmCount] = useState<number | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [settingsLoaded, setSettingsLoaded] = useState(!desktop);
  const [statusLoaded, setStatusLoaded] = useState(!desktop);
  const loading = desktop && (!settingsLoaded || !statusLoaded);
  const translate = useRef(t);
  translate.current = t;

  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    void clipboardStatus().then((current) => {
      if (alive) setStatus(current);
    }).catch(() => undefined).finally(() => {
      if (alive) setStatusLoaded(true);
    });
    void clipboardGetSettings()
      .then((current) => {
        if (!alive) return;
        setDraft({
          ...current,
          copyOnSelect: current.copyOnSelect !== false,
          pasteAutomatically: current.pasteAutomatically !== false,
          pinShortcut: current.pinShortcut || DEFAULT_PIN_SHORTCUT,
          deleteShortcut: current.deleteShortcut || DEFAULT_DELETE_SHORTCUT,
        });
        setTextMb(Number(bytesToMegabytes(current.maxTextBytes)));
        setImageMb(Number(bytesToMegabytes(current.maxImageBytes)));
        setExcluded(current.excludedApps.join("\n"));
        setExcludedApps(current.excludedApps);
        setIgnored((current.ignoredTypes ?? []).join("\n"));
      })
      .catch((caught) => {
        if (!alive) return;
        setError(clipboardErrorText(translate.current, toClipboardError(caught)));
      })
      .finally(() => {
        if (alive) setSettingsLoaded(true);
      });
    return () => {
      alive = false;
    };
  }, [desktop]);

  const payload = useMemo(() => {
    if (!draft || !statusLoaded || textMb == null || imageMb == null) return null;
    const maxItems = Number(draft.maxItems);
    const retentionDays = Number(draft.retentionDays);
    const maxTextBytes = megabytesToBytes(String(textMb));
    const maxImageBytes = megabytesToBytes(String(imageMb));
    if (!Number.isFinite(maxItems) || !Number.isFinite(retentionDays) || !Number.isFinite(maxTextBytes) || !Number.isFinite(maxImageBytes)) return null;
    const next: ClipboardSettings = {
      ...draft,
      maxItems,
      retentionDays,
      maxTextBytes,
      maxImageBytes,
      excludedApps: status?.canReadSourceApp ? excludedApps : excluded.split("\n"),
      ignoredTypes: ignored.split("\n"),
    };
    return next;
  }, [draft, excluded, excludedApps, ignored, imageMb, status, statusLoaded, textMb]);
  const payloadRef = useRef(payload);
  payloadRef.current = payload;
  const confirmedKey = useRef("");

  useAutosave(payload, async (next, isCurrent) => {
    try {
      await clipboardUpdateSettings(toSettingsUpdate(next));
      if (!isCurrent()) return false;
      setConfirmCount(null);
      setError("");
      return true;
    } catch (caught) {
      if (!isCurrent()) return false;
      if (caught instanceof ClipboardRequestError && caught.code === "needs_confirm") {
        if (confirmedKey.current === JSON.stringify(next)) {
          setConfirmCount(null);
          setError("");
          return true;
        }
        setConfirmCount(Number(caught.params.count ?? 0));
        setError("");
        return true;
      }
      if (caught instanceof ClipboardRequestError) setError(clipboardErrorText(translate.current, caught));
      else setError(translate.current("zdx.text.clipboard.error.unknown"));
      return false;
    }
  });

  async function confirmPrune() {
    const next = payloadRef.current;
    if (!next) return;
    setConfirming(true);
    try {
      await clipboardUpdateSettings(toSettingsUpdate(next, true));
      confirmedKey.current = JSON.stringify(next);
      setConfirmCount(null);
      setError("");
    } catch (caught) {
      if (caught instanceof ClipboardRequestError && caught.code === "needs_confirm") {
        setConfirmCount(Number(caught.params.count ?? 0));
        setError("");
      } else if (caught instanceof ClipboardRequestError) {
        setError(clipboardErrorText(translate.current, caught));
      } else {
        setError(translate.current("zdx.text.clipboard.error.unknown"));
      }
    } finally {
      setConfirming(false);
    }
  }

  return (
    <Flex vertical gap={16}>
      <div className="min-w-0">
        <Typography.Paragraph type="secondary" style={{ marginBottom: 0 }}>{t("zdx.text.clipboard.settingsDescription")}</Typography.Paragraph>
        {status && <Typography.Paragraph type="secondary" style={{ marginBottom: 0, wordBreak: "break-all" }}>{t("zdx.text.clipboard.storagePath", { path: status.storagePath })}</Typography.Paragraph>}
      </div>

      {confirmCount !== null && (
        <Alert
          type="warning"
          showIcon
          message={t("zdx.text.clipboard.confirmPrune", { count: confirmCount })}
          action={<Button size="small" type="primary" loading={confirming} onClick={() => void confirmPrune()}>{t("zdx.label.clipboard.confirm")}</Button>}
        />
      )}
      {status?.platform.includes("macos") && <Alert type="info" showIcon message={t("zdx.text.clipboard.macosAccess")} />}
      {error && <Alert type="error" showIcon message={error} />}
      {!desktop && <Alert type="info" showIcon message={t("zdx.text.clipboard.error.desktop_only")} />}
      {desktop && loading && (
        <Flex align="center" gap={8}>
          <Spin size="small" />
          <Typography.Text type="secondary">{t("zdx.text.clipboard.loading")}</Typography.Text>
        </Flex>
      )}

      {desktop && draft && statusLoaded && (
        <Flex vertical gap={24}>
            <SettingGroup title={t("zdx.label.clipboard.groupRecord")}>
              <SettingRow label={t("zdx.label.clipboard.recordText")}>
                <Switch aria-label={t("zdx.label.clipboard.recordText")} checked={draft.recordText} onChange={(checked) => setDraft({ ...draft, recordText: checked })} />
              </SettingRow>
              <SettingRow label={t("zdx.label.clipboard.recordImage")}>
                <Switch aria-label={t("zdx.label.clipboard.recordImage")} checked={draft.recordImage} onChange={(checked) => setDraft({ ...draft, recordImage: checked })} />
              </SettingRow>
              <SettingRow label={t("zdx.label.clipboard.recordFiles")}>
                <Switch aria-label={t("zdx.label.clipboard.recordFiles")} checked={draft.recordFiles} onChange={(checked) => setDraft({ ...draft, recordFiles: checked })} />
              </SettingRow>
              <SettingRow label={t("zdx.label.clipboard.skipSensitive")}>
                <Switch aria-label={t("zdx.label.clipboard.skipSensitive")} checked={draft.skipSensitive} onChange={(checked) => setDraft({ ...draft, skipSensitive: checked })} />
              </SettingRow>
              <SettingRow stacked label={t("zdx.label.clipboard.ignoredTypes")} description={t("zdx.text.clipboard.ignoredTypes")}>
                <Input.TextArea aria-label={t("zdx.label.clipboard.ignoredTypes")} value={ignored} onChange={(event) => setIgnored(event.target.value)} placeholder={t("zdx.placeholder.clipboard.ignoredTypes")} rows={3} />
              </SettingRow>
            </SettingGroup>

            <SettingGroup title={t("zdx.label.clipboard.groupBehavior")}>
              <SettingRow label={t("zdx.label.clipboard.copyOnSelect")} description={t("zdx.text.clipboard.copyOnSelect")}>
                <Switch aria-label={t("zdx.label.clipboard.copyOnSelect")} checked={draft.copyOnSelect} onChange={(checked) => setDraft({ ...draft, copyOnSelect: checked })} />
              </SettingRow>
              <SettingRow label={t("zdx.label.clipboard.pasteAutomatically")} description={t("zdx.text.clipboard.pasteAutomatically", { option: status?.platform.includes("macos") ? "Option" : "Alt" })}>
                <Switch aria-label={t("zdx.label.clipboard.pasteAutomatically")} checked={draft.pasteAutomatically} onChange={(checked) => setDraft({ ...draft, pasteAutomatically: checked })} />
              </SettingRow>
            </SettingGroup>

            <SettingGroup title={t("zdx.label.clipboard.groupStorage")}>
              <SettingRow label={t("zdx.label.clipboard.maxItems")} description={t("zdx.text.clipboard.maxItems")}>
                <InputNumber aria-label={t("zdx.label.clipboard.maxItems")} min={1} max={5000} style={{ width: 140 }} value={draft.maxItems} onChange={(value) => setDraft({ ...draft, maxItems: typeof value === "number" ? value : draft.maxItems })} />
              </SettingRow>
              <SettingRow label={t("zdx.label.clipboard.retentionDays")} description={t("zdx.text.clipboard.retentionZero")}>
                <InputNumber aria-label={t("zdx.label.clipboard.retentionDays")} min={0} max={3650} style={{ width: 140 }} value={draft.retentionDays} onChange={(value) => {
                  if (typeof value === "number") setDraft({ ...draft, retentionDays: value });
                }} />
              </SettingRow>
              <SettingRow label={t("zdx.label.clipboard.maxText")}>
                <InputNumber aria-label={t("zdx.label.clipboard.maxText")} min={0} style={{ width: 140 }} value={textMb} onChange={(value) => {
                  if (typeof value === "number") setTextMb(value);
                }} />
              </SettingRow>
              <SettingRow label={t("zdx.label.clipboard.maxImage")}>
                <InputNumber aria-label={t("zdx.label.clipboard.maxImage")} min={0} style={{ width: 140 }} value={imageMb} onChange={(value) => {
                  if (typeof value === "number") setImageMb(value);
                }} />
              </SettingRow>
            </SettingGroup>

            <SettingGroup title={t("zdx.label.clipboard.groupShortcuts")}>
              <SettingRow label={t("zdx.label.clipboard.shortcut")} description={t("zdx.text.clipboard.shortcutHelp")}>
                <ShortcutField value={draft.shortcut} macos={status?.platform.includes("macos") ?? false} onChange={(shortcut) => setDraft({ ...draft, shortcut })} />
              </SettingRow>
              <SettingRow label={t("zdx.label.clipboard.pinShortcut")} description={t("zdx.text.clipboard.pinShortcut")}>
                <ShortcutField label={t("zdx.label.clipboard.pinShortcut")} value={draft.pinShortcut} macos={status?.platform.includes("macos") ?? false} onChange={(pinShortcut) => setDraft({ ...draft, pinShortcut })} />
              </SettingRow>
              <SettingRow label={t("zdx.label.clipboard.deleteShortcut")} description={t("zdx.text.clipboard.deleteShortcut")}>
                <ShortcutField label={t("zdx.label.clipboard.deleteShortcut")} value={draft.deleteShortcut} macos={status?.platform.includes("macos") ?? false} onChange={(deleteShortcut) => setDraft({ ...draft, deleteShortcut })} />
              </SettingRow>
            </SettingGroup>

            <SettingGroup title={t("zdx.label.clipboard.groupExclude")}>
              <SettingRow
                stacked
                label={t("zdx.label.clipboard.excludedApps")}
                description={
                  <>
                    {status?.canReadSourceApp ? <div>{t("zdx.text.clipboard.excludedPicker")}</div> : status ? <div>{t("zdx.text.clipboard.excludedHelp")}</div> : null}
                    {status?.platform.includes("wayland") && <div>{t("zdx.text.clipboard.excludedWayland")}</div>}
                  </>
                }
              >
                {status == null ? (
                  <Spin size="small" />
                ) : status.canReadSourceApp ? (
                  <ExcludedAppsField value={excludedApps} onChange={setExcludedApps} />
                ) : (
                  <Input.TextArea aria-label={t("zdx.label.clipboard.excludedApps")} value={excluded} onChange={(event) => setExcluded(event.target.value)} placeholder={t("zdx.placeholder.clipboard.excludedApps")} rows={3} />
                )}
              </SettingRow>
            </SettingGroup>
          </Flex>
      )}
    </Flex>
  );
}
