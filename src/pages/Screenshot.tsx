import { ScreenshotRequestError, openScreenRecordingSettings, screenshotRequestAccess, screenshotStart, screenshotStatus, toScreenshotError } from "@/api/screenshot";
import type { ShotStatus } from "@/types/screenshot";
import { isDesktop } from "@/utils/desktop";
import { screenshotErrorText, screenshotReasonText } from "@/utils/screenshotMessage";

export default function ScreenshotPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const desktop = isDesktop();
  const [status, setStatus] = useState<ShotStatus | null>(null);
  const [notice, setNotice] = useState("");
  const [relaunch, setRelaunch] = useState(false);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    void screenshotStatus().then((current) => {
      if (alive) setStatus(current);
    }).catch((error) => {
      if (alive) setNotice(screenshotErrorText(t, toScreenshotError(error)));
    });
    return () => {
      alive = false;
    };
  }, [desktop, t]);

  async function start() {
    setBusy(true);
    setNotice("");
    setRelaunch(false);
    try {
      await screenshotStart();
    } catch (error) {
      if (error instanceof ScreenshotRequestError) {
        setNotice(screenshotErrorText(t, error));
        setRelaunch(error.params.reason === "relaunch_required");
      }
      const current = await screenshotStatus().catch(() => null);
      if (current) setStatus(current);
    } finally {
      setBusy(false);
    }
  }

  async function allow() {
    setBusy(true);
    try {
      const current = await screenshotRequestAccess();
      setStatus(current);
      if (!current.screenRecording && current.platform === "macos") await openScreenRecordingSettings();
    } catch (error) {
      if (error instanceof ScreenshotRequestError) setNotice(screenshotErrorText(t, error));
    } finally {
      setBusy(false);
    }
  }

  const reason = screenshotReasonText(t, status?.captureReason ?? null);
  const blocked = status?.captureReason === "wayland";

  return (
    <Flex vertical gap={16} className="h-full min-h-0 flex-1 overflow-auto">
      <Flex justify="space-between" align="flex-start" gap={16} wrap>
        <div>
          <Typography.Title level={3} style={{ margin: 0 }}>{t("zdx.label.screenshot.title")}</Typography.Title>
          <Typography.Paragraph type="secondary" style={{ marginBottom: 0 }}>{t("zdx.text.screenshot.description")}</Typography.Paragraph>
        </div>
        {desktop && (
          <Space wrap>
            <Button type="primary" onClick={() => void start()} loading={busy} disabled={blocked}>{t("zdx.label.screenshot.start")}</Button>
            <Button onClick={() => navigate("/settings/screenshot")}>{t("zdx.label.screenshot.settings")}</Button>
          </Space>
        )}
      </Flex>
      {!desktop && <Alert type="info" showIcon message={t("zdx.text.screenshot.desktopOnly")} />}
      {desktop && status && (
        <Alert
          type={status.canCapture ? "info" : "warning"}
          showIcon
          message={status.canCapture ? t("zdx.text.screenshot.ready") : t("zdx.text.screenshot.unavailable")}
          description={reason || undefined}
        />
      )}
      {desktop && status?.platform === "macos" && !status.screenRecording && (
        <Alert
          type="warning"
          showIcon
          message={relaunch ? t("zdx.text.screenshot.relaunch") : t("zdx.text.screenshot.permission")}
          action={relaunch ? undefined : <Button size="small" onClick={() => void allow()} disabled={busy}>{t("zdx.label.screenshot.allow")}</Button>}
        />
      )}
      {notice && <Alert type="error" showIcon message={notice} />}
      {desktop && (
        <Flex vertical gap={8}>
          <Typography.Text type="secondary">{status?.canOcr ? t("zdx.text.screenshot.ocrHelp") : t("zdx.text.screenshot.ocrUnsupported")}</Typography.Text>
          <Flex align="center" gap={12}>
            <Button onClick={() => void start()} disabled={blocked || busy}>{t("zdx.label.screenshot.record")}</Button>
            <Typography.Text type="secondary">{t("zdx.text.screenshot.recordEntry")}</Typography.Text>
          </Flex>
        </Flex>
      )}
      <Typography.Text type="secondary">{t("zdx.text.screenshot.local")}</Typography.Text>
    </Flex>
  );
}
