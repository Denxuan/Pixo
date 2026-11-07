import { ClipboardRequestError } from "@/api/clipboard";
import type { ClipboardSettings } from "@/types/clipboard";
import { clipboardErrorText } from "@/utils/clipboardMessage";
import { bytesToMegabytes, megabytesToBytes } from "@/utils/time";

export default function SettingsDialog({
  open,
  settings,
  onClose,
  onSave,
}: {
  open: boolean;
  settings: ClipboardSettings | null;
  onClose: () => void;
  onSave: (settings: ClipboardSettings, confirm: boolean) => Promise<void>;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState<ClipboardSettings | null>(settings);
  const [textMb, setTextMb] = useState("1");
  const [imageMb, setImageMb] = useState("20");
  const [excluded, setExcluded] = useState("");
  const [error, setError] = useState("");
  const [confirmCount, setConfirmCount] = useState<number | null>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!open || !settings) return;
    setDraft(settings);
    setTextMb(bytesToMegabytes(settings.maxTextBytes));
    setImageMb(bytesToMegabytes(settings.maxImageBytes));
    setExcluded(settings.excludedApps.join("\n"));
    setError("");
    setConfirmCount(null);
  }, [open, settings]);

  async function save(confirm: boolean) {
    if (!draft) return;
    const maxItems = Number(draft.maxItems);
    const retentionDays = Number(draft.retentionDays);
    const maxTextBytes = megabytesToBytes(textMb);
    const maxImageBytes = megabytesToBytes(imageMb);
    if (!Number.isFinite(maxItems) || !Number.isFinite(retentionDays) || !Number.isFinite(maxTextBytes) || !Number.isFinite(maxImageBytes)) {
      setError(t("zdx.text.clipboard.error.invalid_input"));
      return;
    }
    const next: ClipboardSettings = {
      ...draft,
      maxItems,
      retentionDays,
      maxTextBytes,
      maxImageBytes,
      excludedApps: excluded.split("\n"),
    };
    setSaving(true);
    try {
      await onSave(next, confirm);
      setConfirmCount(null);
      onClose();
    } catch (caught) {
      if (caught instanceof ClipboardRequestError && caught.code === "needs_confirm") {
        setConfirmCount(Number(caught.params.count ?? 0));
        setError("");
      } else if (caught instanceof ClipboardRequestError) {
        setError(clipboardErrorText(t, caught));
      } else {
        setError(t("zdx.text.clipboard.error.unknown"));
      }
    } finally {
      setSaving(false);
    }
  }

  return (
    <Modal
      open={open}
      title={t("zdx.label.clipboard.settings")}
      okText={t("zdx.label.clipboard.save")}
      cancelText={t("zdx.label.clipboard.cancel")}
      confirmLoading={saving}
      onOk={() => void save(false)}
      onCancel={onClose}
      width={560}
    >
      {draft && (
        <div className="flex flex-col gap-4">
          <label className="flex items-center justify-between gap-3"><span>{t("zdx.label.clipboard.recordText")}</span><Switch checked={draft.recordText} onChange={(checked) => setDraft({ ...draft, recordText: checked })} /></label>
          <label className="flex items-center justify-between gap-3"><span>{t("zdx.label.clipboard.recordImage")}</span><Switch checked={draft.recordImage} onChange={(checked) => setDraft({ ...draft, recordImage: checked })} /></label>
          <label className="flex items-center justify-between gap-3"><span>{t("zdx.label.clipboard.recordFiles")}</span><Switch checked={draft.recordFiles} onChange={(checked) => setDraft({ ...draft, recordFiles: checked })} /></label>
          <label className="flex items-center justify-between gap-3"><span>{t("zdx.label.clipboard.skipSensitive")}</span><Switch checked={draft.skipSensitive} onChange={(checked) => setDraft({ ...draft, skipSensitive: checked })} /></label>
          <label className="flex flex-col gap-1"><span>{t("zdx.label.clipboard.maxItems")}</span><input value={draft.maxItems} onChange={(event) => setDraft({ ...draft, maxItems: Number(event.target.value) })} type="number" min={1} max={5000} className="rounded-lg border border-[#d6deeb] bg-white px-3 py-2 dark:border-[#46546b] dark:bg-[#202b40]" /></label>
          <label className="flex flex-col gap-1"><span>{t("zdx.label.clipboard.retentionDays")}</span><input value={draft.retentionDays} onChange={(event) => setDraft({ ...draft, retentionDays: Number(event.target.value) })} type="number" min={0} max={3650} className="rounded-lg border border-[#d6deeb] bg-white px-3 py-2 dark:border-[#46546b] dark:bg-[#202b40]" /><span className="text-xs text-[#667491] dark:text-[#aab7cf]">{t("zdx.text.clipboard.retentionZero")}</span></label>
          <label className="flex flex-col gap-1"><span>{t("zdx.label.clipboard.maxText")}</span><input value={textMb} onChange={(event) => setTextMb(event.target.value)} inputMode="decimal" className="rounded-lg border border-[#d6deeb] bg-white px-3 py-2 dark:border-[#46546b] dark:bg-[#202b40]" /></label>
          <label className="flex flex-col gap-1"><span>{t("zdx.label.clipboard.maxImage")}</span><input value={imageMb} onChange={(event) => setImageMb(event.target.value)} inputMode="decimal" className="rounded-lg border border-[#d6deeb] bg-white px-3 py-2 dark:border-[#46546b] dark:bg-[#202b40]" /></label>
          <label className="flex flex-col gap-1"><span>{t("zdx.label.clipboard.shortcut")}</span><Input value={draft.shortcut} onChange={(event) => setDraft({ ...draft, shortcut: event.target.value })} placeholder={t("zdx.placeholder.clipboard.shortcut")} /><span className="text-xs text-[#667491] dark:text-[#aab7cf]">{t("zdx.text.clipboard.shortcutHelp")}</span></label>
          <label className="flex flex-col gap-1"><span>{t("zdx.label.clipboard.excludedApps")}</span><Input.TextArea value={excluded} onChange={(event) => setExcluded(event.target.value)} placeholder={t("zdx.placeholder.clipboard.excludedApps")} rows={4} /><span className="text-xs text-[#667491] dark:text-[#aab7cf]">{t("zdx.text.clipboard.excludedHelp")}</span></label>
          {confirmCount !== null && <p className="text-sm text-[#bb3d4a] dark:text-[#ff8b98]">{t("zdx.text.clipboard.confirmPrune", { count: confirmCount })} <Button type="link" onClick={() => void save(true)}>{t("zdx.label.clipboard.confirm")}</Button></p>}
          {error && <p role="alert" className="text-sm text-[#bb3d4a] dark:text-[#ff8b98]">{error}</p>}
        </div>
      )}
    </Modal>
  );
}
