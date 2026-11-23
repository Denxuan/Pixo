import { Channel } from "@tauri-apps/api/core";

import { DockerRequestError, dockerComposeLogs, dockerContainerLogs, dockerTaskStop } from "@/api/docker";
import type { DockerLogEvent } from "@/types/docker";
import { dockerErrorText } from "@/utils/dockerMessage";

type LogLine = { stream: string; text: string };

export default function LogsDialog({
  endpointId,
  target,
  onClose,
}: {
  endpointId: number;
  target: { name: string; containerId?: string } | null;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [lines, setLines] = useState<LogLine[]>([]);
  const [error, setError] = useState("");
  const scroller = useRef<HTMLDivElement>(null);
  const stick = useRef(true);

  useEffect(() => {
    if (!target) return;
    const events = new Channel<DockerLogEvent>();
    const tails = { stdout: "", stderr: "" };
    let task = 0;
    let stop = false;
    const push = (stream: string, text: string) => {
      setLines((current) => {
        const next = current.concat({ stream, text });
        return next.length > 4000 ? next.slice(next.length - 4000) : next;
      });
    };
    events.onmessage = (event) => {
      if (stop) return;
      if (event.type === "closed") {
        if (tails.stdout) push("stdout", tails.stdout);
        if (tails.stderr) push("stderr", tails.stderr);
        tails.stdout = "";
        tails.stderr = "";
        if (event.code !== "ok") setError(dockerErrorText(t, { code: event.code, params: event.params ?? {} }));
        return;
      }
      const key = event.stream === "stderr" ? "stderr" : "stdout";
      const parts = `${tails[key]}${event.text}`.split("\n");
      tails[key] = parts.pop() ?? "";
      for (const text of parts) push(event.stream, text);
    };
    const started = target.containerId
      ? dockerContainerLogs(endpointId, target.containerId, events)
      : dockerComposeLogs(endpointId, target.name, events);
    void started.then((id) => {
      if (stop) {
        void dockerTaskStop(id).catch(() => undefined);
        return;
      }
      task = id;
    }).catch((caught: unknown) => {
      if (!stop && caught instanceof DockerRequestError) setError(dockerErrorText(t, caught));
    });
    return () => {
      stop = true;
      if (task) void dockerTaskStop(task).catch(() => undefined);
      setLines([]);
      setError("");
    };
  }, [endpointId, target, t]);

  useEffect(() => {
    const node = scroller.current;
    if (!node || !stick.current) return;
    node.scrollTop = node.scrollHeight;
  }, [lines, error]);

  return (
    <Modal
      title={t("zdx.text.docker.logsTitle", { name: target?.name ?? "" })}
      open={target !== null}
      centered
      width={960}
      footer={null}
      onCancel={onClose}
      destroyOnHidden
      styles={{
        wrapper: { overflow: "hidden" },
        container: { display: "flex", flexDirection: "column", maxHeight: "calc(100vh - 48px)" },
        header: { flexShrink: 0 },
        body: { flex: "1 1 auto", minHeight: 0, overflow: "hidden", padding: 0 },
      }}
    >
      <div
        ref={scroller}
        className="overflow-auto px-4 py-3 font-mono text-[13px] leading-6"
        style={{ height: "min(640px, calc(100vh - 160px))", background: "#0c0c0c", color: "#d4d4d4" }}
        onScroll={(event) => {
          const node = event.currentTarget;
          stick.current = node.scrollHeight - node.scrollTop - node.clientHeight < 48;
        }}
      >
        {lines.length === 0 && !error && <div style={{ color: "#8a8a8a" }}>{t("zdx.text.docker.logsEmpty")}</div>}
        {lines.map((line, index) => (
          <div key={index} className="whitespace-pre-wrap break-words" style={{ color: lineColor(line) }}>{line.text}</div>
        ))}
        {error && <div className="whitespace-pre-wrap break-words" style={{ color: "#f48771" }}>{error}</div>}
      </div>
    </Modal>
  );
}

function lineColor(line: LogLine) {
  const text = line.text.toUpperCase();
  if (text.includes(" ERROR") || text.startsWith("ERROR") || text.includes("ERROR:")) return "#f48771";
  if (text.includes("WARN") || line.stream === "stderr") return "#d7ba7d";
  return "#d4d4d4";
}
