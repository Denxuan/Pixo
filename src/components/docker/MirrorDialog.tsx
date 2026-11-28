import { DockerRequestError, dockerMirrors, dockerSetMirrors } from "@/api/docker";
import { dockerErrorText } from "@/utils/dockerMessage";

type MirrorRow = {
  id: number;
  value: string;
};

const MAX_MIRRORS = 8;

export default function MirrorDialog({
  endpointId,
  open,
  onClose,
  onSaved,
  onBusy,
}: {
  endpointId: number;
  open: boolean;
  onClose: () => void;
  onSaved: () => void;
  onBusy?: (busy: boolean) => void;
}) {
  const { t } = useTranslation();
  const [rows, setRows] = useState<MirrorRow[]>([]);
  const [loaded, setLoaded] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");
  const nextId = useRef(1);

  useEffect(() => {
    if (!open) return;
    let alive = true;
    setError("");
    setSaving(false);
    setLoading(true);
    void dockerMirrors(endpointId)
      .then((mirrors) => {
        if (!alive) return;
        const cleaned = cleanMirrors(mirrors);
        setLoaded(cleaned);
        setRows(cleaned.length ? cleaned.map((value) => ({ id: nextId.current++, value })) : [{ id: nextId.current++, value: "" }]);
      })
      .catch((caught: unknown) => {
        if (!alive) return;
        setRows([{ id: nextId.current++, value: "" }]);
        setLoaded([]);
        if (caught instanceof DockerRequestError) setError(dockerErrorText(t, caught));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [open, endpointId, t]);

  function addRow() {
    setRows((current) => current.length >= MAX_MIRRORS ? current : [...current, { id: nextId.current++, value: "" }]);
  }

  async function save() {
    const mirrors = cleanMirrors(rows.map((row) => row.value));
    if (sameMirrors(mirrors, loaded)) {
      onClose();
      return;
    }
    if (mirrors.length > MAX_MIRRORS || mirrors.some((value) => !mirrorUrl(value)) || hasDuplicate(mirrors)) {
      setError(t("zdx.text.docker.error.invalid_input"));
      return;
    }
    setSaving(true);
    setError("");
    onBusy?.(true);
    try {
      await dockerSetMirrors(endpointId, mirrors);
      onSaved();
    } catch (caught) {
      onBusy?.(false);
      if (caught instanceof DockerRequestError) setError(dockerErrorText(t, caught));
      setSaving(false);
    }
  }

  return (
    <Modal
      title={t("zdx.label.docker.mirrors")}
      open={open}
      centered
      width={560}
      destroyOnHidden
      maskClosable={!saving}
      onCancel={() => { if (!saving) onClose(); }}
      footer={(
        <div className="flex justify-end gap-2">
          <Button disabled={saving} onClick={onClose}>{t("zdx.label.docker.cancel")}</Button>
          <Button type="primary" loading={saving} disabled={loading} onClick={() => void save()}>{t("zdx.label.docker.save")}</Button>
        </div>
      )}
      styles={{
        wrapper: { overflow: "hidden" },
        container: { display: "flex", flexDirection: "column", maxHeight: "calc(100vh - 48px)" },
        header: { flexShrink: 0 },
        body: { flex: "1 1 auto", minHeight: 0, overflowY: "auto", overscrollBehavior: "contain" },
        footer: { flexShrink: 0 },
      }}
    >
      <Flex vertical gap={12}>
        {error && <Alert type="error" showIcon message={error} />}
        <Typography.Text type="secondary">{t("zdx.text.docker.mirrorsHint")}</Typography.Text>
        <Alert type="warning" showIcon message={t("zdx.text.docker.mirrorsRestart")} />
        {loading ? <Spin /> : rows.map((row) => (
          <div key={row.id} className="flex items-center gap-2">
            <Input
              className="min-w-0 flex-1"
              value={row.value}
              disabled={saving}
              aria-label={t("zdx.label.docker.mirrors")}
              placeholder={t("zdx.placeholder.docker.mirror")}
              onChange={(event) => setRows((current) => current.map((item) => item.id === row.id ? { ...item, value: event.target.value } : item))}
            />
            <Button
              type="text"
              disabled={saving}
              aria-label={t("zdx.label.docker.removeMirror")}
              icon={<CloseOutlined />}
              onClick={() => setRows((current) => current.filter((item) => item.id !== row.id))}
            />
          </div>
        ))}
        {!loading && (
          <Button className="self-start" disabled={saving || rows.length >= MAX_MIRRORS} icon={<PlusOutlined />} onClick={addRow}>
            {t("zdx.label.docker.addMirror")}
          </Button>
        )}
      </Flex>
    </Modal>
  );
}

function cleanMirrors(values: string[]): string[] {
  return values.map((value) => value.trim().replace(/\/+$/, "")).filter((value) => value.length > 0);
}

function sameMirrors(next: string[], previous: string[]): boolean {
  return next.length === previous.length && next.every((value, index) => value === previous[index]);
}

function hasDuplicate(values: string[]): boolean {
  const seen = new Set<string>();
  for (const value of values) {
    const key = value.toLowerCase();
    if (seen.has(key)) return true;
    seen.add(key);
  }
  return false;
}

function mirrorUrl(value: string): boolean {
  if (value.length === 0 || value.length > 200 || /[\s\u0000-\u001f\u007f]/.test(value)) return false;
  const match = /^(https?:\/\/)([^?#@\\\s]+)$/.exec(value);
  if (!match || value.includes("..")) return false;
  const host = match[2].split("/")[0];
  if (!host || host.startsWith(".") || host.endsWith(".") || host.startsWith("-") || host.endsWith("-")) return false;
  if (!/[A-Za-z0-9]/.test(host)) return false;
  if (!/^[A-Za-z0-9.:\-[\]]+$/.test(host)) return false;
  const path = match[2].slice(host.length);
  return /^[A-Za-z0-9./\-_~:]*$/.test(path);
}
