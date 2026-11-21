import { ScreenshotRequestError, openScreenRecordingSettings, screenshotRequestAccess, screenshotStart, screenshotStatus, toScreenshotError } from "@/api/screenshot";
import Page from "@/components/Page";
import type { ShotStatus } from "@/types/screenshot";
import { isDesktop } from "@/utils/desktop";
import { screenshotErrorText, screenshotReasonText } from "@/utils/screenshotMessage";

function ActionCard({ icon, title, description, action }: { icon: ReactNode; title: string; description: string; action?: ReactNode }) {
  const { token } = antdTheme.useToken();
  return (
    <div className="flex flex-col gap-3 p-5" style={{ border: `1px solid ${token.colorBorderSecondary}`, borderRadius: token.borderRadiusLG, background: token.colorBgContainer }}>
      <span className="flex h-10 w-10 items-center justify-center text-lg" style={{ borderRadius: token.borderRadius, background: token.colorPrimaryBg, color: token.colorPrimary }} aria-hidden="true">{icon}</span>
      <div className="flex flex-1 flex-col gap-1">
        <Typography.Text strong>{title}</Typography.Text>
        <Typography.Text type="secondary" style={{ fontSize: token.fontSizeSM, lineHeight: 1.6 }}>{description}</Typography.Text>
      </div>
      {action && <div>{action}</div>}
    </div>
  );
}

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
    <Page
      title={t("zdx.label.screenshot.title")}
      description={t("zdx.text.screenshot.description")}
      extra={desktop && (
        <Tooltip title={t("zdx.label.screenshot.settings")}>
          <Button icon={<SettingOutlined />} aria-label={t("zdx.label.screenshot.settings")} onClick={() => navigate("/settings/screenshot")} />
        </Tooltip>
      )}
    >
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
        <div className="grid grid-cols-[repeat(auto-fit,minmax(240px,1fr))] gap-4">
          <ActionCard
            icon={<ScissorOutlined />}
            title={t("zdx.label.screenshot.cardCapture")}
            description={t("zdx.text.screenshot.cardCapture")}
            action={<Button type="primary" onClick={() => void start()} loading={busy} disabled={blocked}>{t("zdx.label.screenshot.start")}</Button>}
          />
          <ActionCard
            icon={<VideoCameraOutlined />}
            title={t("zdx.label.screenshot.cardRecord")}
            description={t("zdx.text.screenshot.recordEntry")}
            action={<Button onClick={() => void start()} disabled={blocked || busy}>{t("zdx.label.screenshot.record")}</Button>}
          />
          <ActionCard
            icon={<FileTextOutlined />}
            title={t("zdx.label.screenshot.cardOcr")}
            description={status?.canOcr ? t("zdx.text.screenshot.ocrHelp") : t("zdx.text.screenshot.ocrUnsupported")}
          />
        </div>
      )}
      <Typography.Text type="secondary" className="text-xs">{t("zdx.text.screenshot.local")}</Typography.Text>
    </Page>
  );
}
