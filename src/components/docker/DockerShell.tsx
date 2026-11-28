import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { Channel } from "@tauri-apps/api/core";

import { DockerRequestError, dockerExecClose, dockerExecOpen, dockerExecResize, dockerExecWrite, type DockerExecEvent } from "@/api/docker";
import { useSshAppearanceStore } from "@/stores/sshAppearance";
import { useSettingsStore } from "@/stores/settings";
import { isDarkTheme } from "@/utils/theme";
import { dockerErrorText } from "@/utils/dockerMessage";
import { resolvePalette, toTerminalTheme, terminalFontFamily } from "@/utils/sshAppearance";
import { terminalBytes } from "@/utils/sshMessage";

const SHELLS = [
  { command: "/bin/sh", label: "sh" },
  { command: "/bin/bash", label: "bash" },
  { command: "/bin/ash", label: "ash" },
  { command: "/bin/zsh", label: "zsh" },
] as const;

const colorScheme = window.matchMedia("(prefers-color-scheme: dark)");
const subscribeScheme = (onChange: () => void) => {
  colorScheme.addEventListener("change", onChange);
  return () => colorScheme.removeEventListener("change", onChange);
};

export default function DockerShell({
  endpointId,
  container,
  onClose,
}: {
  endpointId: number;
  container: { id: string; name: string } | null;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const mode = useSettingsStore((state) => state.mode);
  const systemDark = useSyncExternalStore(subscribeScheme, () => colorScheme.matches);
  const dark = isDarkTheme(mode, systemDark);
  const themeId = useSshAppearanceStore((state) => state.themeId);
  const customPalette = useSshAppearanceStore((state) => state.custom);
  const fontId = useSshAppearanceStore((state) => state.fontId);
  const customFont = useSshAppearanceStore((state) => state.customFont);
  const fontSize = useSshAppearanceStore((state) => state.fontSize);
  const [error, setError] = useState("");
  const [shown, setShown] = useState(false);
  const [command, setCommand] = useState<(typeof SHELLS)[number]["command"]>("/bin/sh");
  const host = useRef<HTMLDivElement>(null);
  const session = useRef(0);

  useEffect(() => {
    setCommand("/bin/sh");
    setError("");
  }, [container?.id]);

  useEffect(() => {
    const node = host.current;
    if (!shown || !container || !node) return;
    const term = new Terminal({
      cursorBlink: true,
      fontSize,
      fontFamily: terminalFontFamily(fontId, customFont),
      theme: toTerminalTheme(resolvePalette(themeId, customPalette, dark)),
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(node);
    fit.fit();
    const output = new Channel<unknown>((message) => {
      const bytes = terminalBytes(message);
      if (bytes && bytes.byteLength > 0) term.write(bytes);
    });
    const events = new Channel<DockerExecEvent>();
    events.onmessage = (event) => {
      if (event.type === "closed" && event.code !== "disconnected") {
        setError(dockerErrorText(t, { code: event.code, params: event.params ?? {} }));
      }
    };
    let closed = false;
    term.onData((data) => {
      const id = session.current;
      if (!id) return;
      void dockerExecWrite(id, Array.from(new TextEncoder().encode(data))).catch(() => undefined);
    });
    term.onResize(({ cols, rows }) => {
      const id = session.current;
      if (id) void dockerExecResize(id, cols, rows).catch(() => undefined);
    });
    void dockerExecOpen(endpointId, container.id, term.cols || 80, term.rows || 24, command, output, events).then((id) => {
      if (closed) {
        void dockerExecClose(id).catch(() => undefined);
        return;
      }
      session.current = id;
      fit.fit();
      void dockerExecResize(id, term.cols, term.rows).catch(() => undefined);
      term.focus();
    }).catch((caught: unknown) => {
      if (caught instanceof DockerRequestError) setError(dockerErrorText(t, caught));
    });
    const observer = new ResizeObserver(() => {
      fit.fit();
    });
    observer.observe(node);
    return () => {
      closed = true;
      observer.disconnect();
      const id = session.current;
      session.current = 0;
      if (id) void dockerExecClose(id).catch(() => undefined);
      term.dispose();
    };
  }, [command, container, customFont, customPalette, dark, endpointId, fontId, fontSize, shown, t, themeId]);

  return (
    <Modal
      open={container !== null}
      title={container ? t("zdx.text.docker.shellTitle", { name: container.name }) : t("zdx.label.docker.shell")}
      footer={null}
      width={860}
      onCancel={onClose}
      afterOpenChange={setShown}
      destroyOnHidden
    >
      <Flex vertical gap={8}>
        <Typography.Text type="secondary">{t("zdx.text.docker.shellHint")}</Typography.Text>
        <Segmented
          block
          value={command}
          aria-label={t("zdx.label.docker.shell")}
          options={SHELLS.map((shell) => ({ value: shell.command, label: shell.label }))}
          onChange={(value) => {
            const next = SHELLS.find((shell) => shell.command === value);
            if (next) {
              setError("");
              setCommand(next.command);
            }
          }}
        />
        {error && <Alert type="error" showIcon message={error} />}
        <div ref={host} className="h-96 overflow-hidden rounded-md" />
      </Flex>
    </Modal>
  );
}
