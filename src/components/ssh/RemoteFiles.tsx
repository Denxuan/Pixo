import { SshRequestError, sshListDir, sshMakeDir, sshRemove, sshRename } from "@/api/ssh";
import type { SshRemoteListing } from "@/types/ssh";
import { formatBytes } from "@/utils/sshTransfer";
import { sshErrorText } from "@/utils/sshMessage";

function validName(name: string) {
  const trimmed = name.trim();
  return trimmed.length > 0 && trimmed.length <= 255 && trimmed !== "." && trimmed !== ".." && !/[\0/\\]/.test(trimmed);
}

function joinRemote(dir: string, name: string) {
  if (!dir || dir === "/") return `/${name}`;
  return `${dir.replace(/\/$/, "")}/${name}`;
}

export default function RemoteFiles({
  open,
  sessionId,
  onClose,
  onDownload,
  onInsert,
}: {
  open: boolean;
  sessionId: number;
  onClose: () => void;
  onDownload: (paths: string[]) => void;
  onInsert: (path: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const [path, setPath] = useState("");
  const [listing, setListing] = useState<SshRemoteListing | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [reload, setReload] = useState(0);
  const [naming, setNaming] = useState<{ mode: "mkdir" | "rename"; value: string; from?: string } | null>(null);

  useEffect(() => {
    if (!open) return;
    setPath("");
    setSelected([]);
    setListing(null);
  }, [open]);

  useEffect(() => {
    if (!open || sessionId === 0) return;
    let cancelled = false;
    setBusy(true);
    setError("");
    sshListDir(sessionId, path)
      .then((next) => {
        if (!cancelled) setListing(next);
      })
      .catch((caught: unknown) => {
        if (!cancelled) setError(caught instanceof SshRequestError ? sshErrorText(t, caught) : t("zdx.text.ssh.error.unknown"));
      })
      .finally(() => {
        if (!cancelled) setBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, path, reload, sessionId, t]);

  function toggle(entryPath: string, checked: boolean) {
    setSelected((current) => checked ? [...current, entryPath] : current.filter((item) => item !== entryPath));
  }

  function refresh() {
    setSelected([]);
    setReload((current) => current + 1);
  }

  async function applyName() {
    if (!naming || !validName(naming.value)) return;
    const directory = listing?.path || path || "/";
    setBusy(true);
    setError("");
    try {
      if (naming.mode === "mkdir") {
        await sshMakeDir(sessionId, joinRemote(directory, naming.value.trim()));
      } else if (naming.from) {
        const parent = naming.from.slice(0, naming.from.lastIndexOf("/")) || "/";
        await sshRename(sessionId, naming.from, joinRemote(parent, naming.value.trim()));
      }
      setNaming(null);
      refresh();
    } catch (caught) {
      setError(caught instanceof SshRequestError ? sshErrorText(t, caught) : t("zdx.text.ssh.error.unknown"));
    } finally {
      setBusy(false);
    }
  }

  function removeSelected() {
    const paths = selected;
    Modal.confirm({
      title: t("zdx.text.ssh.deletePathTitle"),
      content: t("zdx.text.ssh.deletePathBody"),
      okText: t("zdx.label.ssh.deletePath"),
      cancelText: t("zdx.label.ssh.cancel"),
      okButtonProps: { danger: true },
      onOk: async () => {
        try {
          await sshRemove(sessionId, paths);
          refresh();
        } catch (caught) {
          setError(caught instanceof SshRequestError ? sshErrorText(t, caught) : t("zdx.text.ssh.error.unknown"));
        }
      },
    });
  }

  return (
    <Modal
      title={t("zdx.text.ssh.filesTitle")}
      open={open}
      centered
      width={640}
      footer={(
        <Flex gap={8} wrap="wrap" justify="flex-end">
          <Button onClick={() => setNaming({ mode: "mkdir", value: "" })}>{t("zdx.label.ssh.mkdir")}</Button>
          <Button
            disabled={selected.length !== 1}
            onClick={() => {
              const from = selected[0];
              const name = from.slice(from.lastIndexOf("/") + 1);
              setNaming({ mode: "rename", value: name, from });
            }}
          >
            {t("zdx.label.ssh.rename")}
          </Button>
          <Button danger disabled={selected.length === 0} onClick={removeSelected}>{t("zdx.label.ssh.deletePath")}</Button>
          <Button disabled={!listing?.path} onClick={() => listing && onInsert(listing.path)}>{t("zdx.label.ssh.insertPath")}</Button>
          <Button onClick={onClose}>{t("zdx.label.ssh.cancel")}</Button>
          <Button type="primary" disabled={selected.length === 0} onClick={() => onDownload(selected)}>{t("zdx.label.ssh.download")}</Button>
        </Flex>
      )}
      destroyOnHidden
      styles={{ body: { maxHeight: "calc(100vh - 220px)", overflowY: "auto" } }}
      onCancel={onClose}
    >
      <Flex vertical gap={12}>
        <Flex gap={8} align="center">
          <Button size="small" disabled={!listing?.parent} onClick={() => listing?.parent && setPath(listing.parent)}>{t("zdx.label.ssh.parentDir")}</Button>
          <Typography.Text code className="min-w-0 flex-1 truncate">{listing?.path || path || "/"}</Typography.Text>
        </Flex>
        {error && <Alert type="error" showIcon message={error} />}
        {busy && <Spin />}
        {!busy && listing && listing.entries.length === 0 && <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("zdx.text.ssh.filesEmpty")} />}
        {listing?.entries.map((entry) => {
          const linked = entry.kind === "link" || entry.kind === "other";
          return (
            <Flex key={entry.path} align="center" gap={8}>
              <Checkbox
                aria-label={entry.name}
                disabled={linked}
                checked={selected.includes(entry.path)}
                onChange={(event) => toggle(entry.path, event.target.checked)}
              />
              <Button
                type="text"
                className="min-w-0 flex-1 justify-start px-1"
                disabled={entry.kind !== "dir"}
                onClick={() => {
                  if (entry.kind === "dir") setPath(entry.path);
                }}
              >
                <span className="truncate">{entry.name}</span>
              </Button>
              <span className="shrink-0 text-xs" style={{ opacity: 0.65 }}>
                {entry.kind === "dir" ? t("zdx.label.ssh.kindDir") : linked ? t("zdx.label.ssh.kindLink") : formatBytes(entry.size, i18n.language)}
              </span>
            </Flex>
          );
        })}
      </Flex>
      <Modal
        title={t(naming?.mode === "rename" ? "zdx.text.ssh.renameTitle" : "zdx.text.ssh.mkdirTitle")}
        open={naming !== null}
        okText={t("zdx.label.ssh.confirm")}
        cancelText={t("zdx.label.ssh.cancel")}
        confirmLoading={busy}
        okButtonProps={{ disabled: !naming || !validName(naming.value) }}
        onCancel={() => setNaming(null)}
        onOk={() => void applyName()}
      >
        <Input
          autoFocus
          aria-label={t(naming?.mode === "rename" ? "zdx.placeholder.ssh.newName" : "zdx.placeholder.ssh.dirName")}
          placeholder={t(naming?.mode === "rename" ? "zdx.placeholder.ssh.newName" : "zdx.placeholder.ssh.dirName")}
          value={naming?.value ?? ""}
          onChange={(event) => setNaming((current) => current ? { ...current, value: event.target.value } : current)}
          onPressEnter={() => void applyName()}
        />
      </Modal>
    </Modal>
  );
}
