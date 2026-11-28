import { DockerRequestError, dockerListDirs } from "@/api/docker";
import type { HostDirListing } from "@/types/docker";
import { dockerErrorText } from "@/utils/dockerMessage";

export default function HostFolder({
  open,
  endpointId,
  initial,
  onClose,
  onPick,
}: {
  open: boolean;
  endpointId: number;
  initial: string;
  onClose: () => void;
  onPick: (path: string) => void;
}) {
  const { t } = useTranslation();
  const [path, setPath] = useState("");
  const [draft, setDraft] = useState("");
  const [listing, setListing] = useState<HostDirListing | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    const start = initial.trim();
    const absolute = start.startsWith("/") || /^[A-Za-z]:[\\/]/.test(start);
    const next = absolute ? start : "";
    setPath(next);
    setDraft(next);
    setListing(null);
    setError("");
  }, [open, initial]);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setBusy(true);
    setError("");
    dockerListDirs(endpointId, path)
      .then((next) => {
        if (cancelled) return;
        setListing(next);
        setDraft(next.path);
      })
      .catch((caught: unknown) => {
        if (cancelled) return;
        setListing(null);
        setError(caught instanceof DockerRequestError ? dockerErrorText(t, caught) : t("zdx.text.docker.error.unknown"));
      })
      .finally(() => {
        if (!cancelled) setBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, [endpointId, open, path, t]);

  return (
    <Modal
      title={t("zdx.label.docker.chooseFolder")}
      open={open}
      centered
      width={560}
      zIndex={1200}
      okText={t("zdx.label.docker.useFolder")}
      cancelText={t("zdx.label.docker.cancel")}
      okButtonProps={{ disabled: !listing || busy }}
      onOk={() => {
        if (!listing || busy) return;
        const next = draft.trim();
        if (next !== listing.path) {
          setPath(next);
          return;
        }
        onPick(listing.path);
      }}
      onCancel={onClose}
      destroyOnHidden
      styles={{ body: { maxHeight: "calc(100vh - 220px)", overflowY: "auto" } }}
    >
      <Flex vertical gap={12}>
        <Flex gap={8}>
          <Button disabled={busy || !listing?.parent} onClick={() => listing?.parent && setPath(listing.parent)}>
            {t("zdx.label.docker.parentFolder")}
          </Button>
          <Input
            className="min-w-0 flex-1"
            value={draft}
            aria-label={t("zdx.label.docker.hostPath")}
            onChange={(event) => setDraft(event.target.value)}
            onPressEnter={(event) => {
              event.preventDefault();
              event.stopPropagation();
              setPath(draft.trim());
            }}
          />
        </Flex>
        {error && <Alert type="error" showIcon message={error} />}
        {busy && <Spin />}
        {!busy && listing && listing.entries.length === 0 && (
          <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("zdx.text.docker.folderEmpty")} />
        )}
        {!busy && listing && listing.entries.length > 0 && (
          <Flex vertical gap={2}>
            {listing.entries.map((entry) => (
              <Button
                key={entry.path}
                type="text"
                className="w-full min-w-0 justify-start"
                icon={<FolderOutlined />}
                onClick={() => setPath(entry.path)}
              >
                <span className="truncate">{entry.name}</span>
              </Button>
            ))}
          </Flex>
        )}
      </Flex>
    </Modal>
  );
}
