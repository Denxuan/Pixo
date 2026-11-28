import type { InputRef } from "antd";

import { clipboardCopy, clipboardList, clipboardSetPaused, clipboardShowPanel, clipboardStatus } from "@/api/clipboard";
import { dockerEndpoints } from "@/api/docker";
import { screenshotLibrary, screenshotLibraryCopy, screenshotStart, screenshotStartMode } from "@/api/screenshot";
import { sshHosts } from "@/api/ssh";
import { arrangeLauncherHits, matchCommands, type CommandId } from "@/commands/registry";
import { isDesktop } from "@/utils/desktop";

type Hit = {
  key: string;
  group: string;
  title: string;
  detail: string;
  run: () => void | Promise<void>;
};

export default function Launcher({
  open,
  clipboardOn,
  screenshotOn,
  sshOn,
  dockerOn,
  onClose,
}: {
  open: boolean;
  clipboardOn: boolean | null;
  screenshotOn: boolean | null;
  sshOn: boolean | null;
  dockerOn: boolean | null;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<Hit[]>([]);
  const [active, setActive] = useState(0);
  const [error, setError] = useState("");
  const input = useRef<InputRef>(null);
  const ticket = useRef(0);

  useEffect(() => {
    if (!open) {
      setQuery("");
      setHits([]);
      setActive(0);
      setError("");
      return;
    }
    const timer = window.setTimeout(() => {
      void load(query);
    }, 120);
    return () => window.clearTimeout(timer);
  }, [open, query, clipboardOn, screenshotOn, sshOn, dockerOn]);

  async function runCommand(id: CommandId) {
    if (id === "screenshot.capture") await screenshotStart();
    else if (id === "screenshot.record") await screenshotStartMode("record");
    else if (id === "clipboard.panel") await clipboardShowPanel();
    else if (id === "clipboard.pause") await clipboardSetPaused(true);
    else if (id === "clipboard.resume") await clipboardSetPaused(false);
    else if (id === "settings.open") navigate("/settings");
    else navigate("/settings/data");
  }

  async function load(text: string) {
    if (!isDesktop()) return;
    const current = ticket.current + 1;
    ticket.current = current;
    const needle = text.trim().toLowerCase();
    const objects: Hit[] = [];
    const tasks: Array<Promise<void>> = [];
    let paused = false;
    if (clipboardOn === true) {
      tasks.push(clipboardStatus().then((status) => {
        paused = status.paused;
      }).catch(() => undefined));
      tasks.push(clipboardList({ query: text.trim(), limit: 6 }).then((page) => {
        for (const item of page.items) {
          const title = item.label || item.preview || item.fileNames[0] || t("zdx.label.clipboard.text");
          objects.push({
            key: `clip-${item.id}`,
            group: t("zdx.label.nav.clipboard"),
            title,
            detail: t("zdx.text.launcher.copy"),
            run: () => clipboardCopy(item.id).then(() => undefined),
          });
        }
      }).catch(() => undefined));
    }
    if (sshOn) {
      tasks.push(sshHosts().then((hosts) => {
        for (const host of hosts) {
          const haystack = `${host.name} ${host.host} ${host.username}`.toLowerCase();
          if (needle && !haystack.includes(needle)) continue;
          objects.push({
            key: `host-${host.id}`,
            group: t("zdx.label.nav.ssh"),
            title: host.name,
            detail: `${host.username}@${host.host}`,
            run: () => navigate("/ssh", { state: { hostId: host.id } }),
          });
          if (objects.filter((item) => item.key.startsWith("host-")).length >= 6) break;
        }
      }).catch(() => undefined));
    }
    if (dockerOn) {
      tasks.push(dockerEndpoints().then((endpoints) => {
        for (const endpoint of endpoints) {
          const haystack = `${endpoint.name} ${endpoint.host ?? ""} ${endpoint.socketPath ?? ""}`.toLowerCase();
          if (needle && !haystack.includes(needle)) continue;
          objects.push({
            key: `docker-${endpoint.id}`,
            group: t("zdx.label.nav.docker"),
            title: endpoint.name,
            detail: endpoint.kind,
            run: () => navigate("/docker", { state: { hostId: endpoint.id } }),
          });
          if (objects.filter((item) => item.key.startsWith("docker-")).length >= 6) break;
        }
      }).catch(() => undefined));
    }
    if (screenshotOn === true) {
      tasks.push(screenshotLibrary(text.trim()).then((items) => {
        for (const item of items.slice(0, 6)) {
          const image = item.kind === "image";
          objects.push({
            key: `shot-${item.id}`,
            group: t("zdx.label.nav.screenshot"),
            title: item.text?.trim() || item.name,
            detail: image ? t("zdx.text.launcher.copy") : t("zdx.text.launcher.open"),
            run: () => {
              if (image) return screenshotLibraryCopy(item.id).then(() => undefined);
              navigate("/screenshot");
            },
          });
        }
      }).catch(() => undefined));
    }
    await Promise.all(tasks);
    if (ticket.current !== current) return;
    const commands = matchCommands({
      query: text,
      clipboardOn: clipboardOn === true,
      clipboardPaused: paused,
      screenshotOn: screenshotOn === true,
    }).map((command) => ({
      key: command.id,
      group: t("zdx.label.launcher.actions"),
      title: t(command.titleKey),
      detail: t(command.detailKey),
      run: () => runCommand(command.id),
    }));
    setHits(arrangeLauncherHits(text, commands, objects));
    setActive(0);
  }

  async function choose(index: number) {
    const hit = hits[index];
    if (!hit) return;
    setError("");
    try {
      await hit.run();
      onClose();
    } catch {
      setError(t("zdx.text.launcher.failed"));
    }
  }

  return (
    <Modal
      open={open}
      title={t("zdx.label.launcher.title")}
      footer={null}
      onCancel={onClose}
      afterOpenChange={(shown) => { if (shown) input.current?.focus(); }}
      destroyOnHidden
    >
      <Flex vertical gap={12}>
        <Input
          ref={input}
          value={query}
          placeholder={t("zdx.placeholder.launcher.query")}
          aria-label={t("zdx.placeholder.launcher.query")}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "ArrowDown") {
              event.preventDefault();
              setActive((value) => Math.min(hits.length - 1, value + 1));
            } else if (event.key === "ArrowUp") {
              event.preventDefault();
              setActive((value) => Math.max(0, value - 1));
            } else if (event.key === "Enter") {
              event.preventDefault();
              void choose(active);
            }
          }}
        />
        {error && <Alert type="error" showIcon message={error} />}
        {hits.length === 0 && <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("zdx.text.launcher.empty")} />}
        <div role="listbox" aria-label={t("zdx.label.launcher.title")} className="flex max-h-80 flex-col gap-1 overflow-auto">
          {hits.map((hit, index) => (
            <button
              key={hit.key}
              type="button"
              role="option"
              aria-selected={index === active}
              className="flex cursor-pointer items-center gap-3 rounded-md border-0 bg-transparent px-2 py-2 text-left"
              style={{ background: index === active ? "var(--ant-color-fill-secondary, rgba(0,0,0,0.06))" : undefined }}
              onMouseEnter={() => setActive(index)}
              onClick={() => void choose(index)}
            >
              <span className="w-16 shrink-0 text-xs opacity-60">{hit.group}</span>
              <span className="min-w-0 flex-1">
                <span className="block truncate">{hit.title}</span>
                <span className="block truncate text-xs opacity-60">{hit.detail}</span>
              </span>
            </button>
          ))}
        </div>
      </Flex>
    </Modal>
  );
}
