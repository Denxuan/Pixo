import { dismissClipboardPanel, launcherOpen } from "@/api/clipboard";
import { screenshotGetSettings, screenshotLibrary, screenshotLibraryCopy, screenshotStartMode, ScreenshotRequestError } from "@/api/screenshot";
import { sshHosts, sshStatus, SshRequestError } from "@/api/ssh";
import type { ShotFile } from "@/types/screenshot";
import type { SshHost } from "@/types/ssh";
import { screenshotErrorText } from "@/utils/screenshotMessage";
import { sshErrorText } from "@/utils/sshMessage";

export default function QuickShelf({ query, onNotice }: { query: string; onNotice: (text: string) => void }) {
  const { t } = useTranslation();
  const { token } = antdTheme.useToken();
  const [shots, setShots] = useState<ShotFile[]>([]);
  const [hosts, setHosts] = useState<SshHost[]>([]);
  const [screenshotOn, setScreenshotOn] = useState(false);
  const [sshOn, setSshOn] = useState(false);

  const load = useCallback(async () => {
    const [shotSettings, ssh, files, savedHosts] = await Promise.all([
      screenshotGetSettings().catch(() => null),
      sshStatus().catch(() => null),
      screenshotLibrary().catch(() => [] as ShotFile[]),
      sshHosts().catch(() => [] as SshHost[]),
    ]);
    setScreenshotOn(shotSettings?.enabled === true);
    setSshOn(ssh?.enabled === true);
    setShots(files);
    setHosts(savedHosts);
  }, []);

  useEffect(() => {
    void load();
    const onShown = () => void load();
    window.addEventListener("clipboard-panel-shown", onShown);
    return () => window.removeEventListener("clipboard-panel-shown", onShown);
  }, [load]);

  const needle = query.trim().toLowerCase();
  const actions = screenshotOn
    ? [
        { id: "capture" as const, label: t("zdx.label.clipboard.quickCapture") },
        { id: "record" as const, label: t("zdx.label.clipboard.quickRecord") },
      ].filter((action) => !needle || action.label.toLowerCase().includes(needle))
    : [];
  const visibleShots = shots
    .filter((item) => !needle || `${item.name} ${item.text ?? ""}`.toLowerCase().includes(needle))
    .slice(0, needle ? 5 : 3);
  const visibleHosts = sshOn && needle
    ? hosts.filter((host) => `${host.name} ${host.host} ${host.username}`.toLowerCase().includes(needle)).slice(0, 5)
    : [];

  async function start(mode: "capture" | "record") {
    try {
      await dismissClipboardPanel();
      await screenshotStartMode(mode);
    } catch (error) {
      if (error instanceof ScreenshotRequestError) onNotice(screenshotErrorText(t, error));
    }
  }

  async function copyShot(id: number) {
    try {
      await screenshotLibraryCopy(id);
      await dismissClipboardPanel();
    } catch (error) {
      if (error instanceof ScreenshotRequestError) onNotice(screenshotErrorText(t, error));
    }
  }

  async function openHost(id: number) {
    try {
      await launcherOpen("/ssh", id);
    } catch (error) {
      if (error instanceof SshRequestError) onNotice(sshErrorText(t, error));
    }
  }

  if (actions.length === 0 && visibleShots.length === 0 && visibleHosts.length === 0) return null;

  return (
    <div className="mb-2 flex flex-col gap-2">
      {actions.length > 0 && (
        <div className="flex gap-2">
          {actions.map((action) => (
            <Button key={action.id} size="small" onClick={() => void start(action.id)}>
              {action.label}
            </Button>
          ))}
        </div>
      )}
      {visibleHosts.length > 0 && (
        <div>
          <div className="px-1 pb-1 text-xs" style={{ color: token.colorTextTertiary }}>{t("zdx.label.clipboard.quickHosts")}</div>
          {visibleHosts.map((host) => (
            <button
              key={host.id}
              type="button"
              className="flex w-full cursor-pointer items-center gap-2 bg-transparent px-2 py-1.5 text-left"
              style={{ borderRadius: token.borderRadius }}
              onClick={() => void openHost(host.id)}
            >
              <CloudServerOutlined />
              <span className="min-w-0 flex-1 truncate">{host.name}</span>
              <span className="truncate text-xs" style={{ color: token.colorTextTertiary }}>{host.username}@{host.host}</span>
            </button>
          ))}
        </div>
      )}
      {visibleShots.length > 0 && (
        <div>
          <div className="px-1 pb-1 text-xs" style={{ color: token.colorTextTertiary }}>{t("zdx.label.clipboard.quickShots")}</div>
          {visibleShots.map((shot) => (
            <button
              key={shot.id}
              type="button"
              className="flex w-full cursor-pointer items-center gap-2 bg-transparent px-2 py-1.5 text-left"
              style={{ borderRadius: token.borderRadius }}
              onClick={() => void copyShot(shot.id)}
            >
              {shot.kind === "video" ? <VideoCameraOutlined /> : <FileImageOutlined />}
              <span className="min-w-0 flex-1 truncate">{shot.text?.trim() || shot.name}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
