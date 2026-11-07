import { clipboardImage } from "@/api/clipboard";
import type { ClipSummary } from "@/types/clipboard";
import { formatBytes, formatRelativeTime } from "@/utils/time";

function kindIcon(kind: string) {
  if (kind === "image") return <FileImageOutlined aria-hidden="true" />;
  if (kind === "files") return <FileOutlined aria-hidden="true" />;
  return <FontSizeOutlined aria-hidden="true" />;
}

function ClipThumb({ id }: { id: number }) {
  const { t } = useTranslation();
  const [url, setUrl] = useState("");

  useEffect(() => {
    let alive = true;
    let objectUrl = "";
    void clipboardImage(id, true).then((bytes) => {
      if (!alive) return;
      const buffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
      objectUrl = URL.createObjectURL(new Blob([buffer], { type: "image/png" }));
      setUrl(objectUrl);
    }).catch(() => {
      if (alive) setUrl("");
    });
    return () => {
      alive = false;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [id]);

  if (!url) return null;
  return <img src={url} alt={t("zdx.label.clipboard.image")} className="h-14 w-14 shrink-0 rounded-lg object-cover" />;
}

export default function HistoryList({
  items,
  activeId,
  onActive,
  onCopy,
  onPin,
  onDelete,
}: {
  items: ClipSummary[];
  activeId: number | null;
  onActive: (id: number) => void;
  onCopy: (id: number) => void;
  onPin: (id: number, pinned: boolean) => void;
  onDelete: (id: number) => void;
}) {
  const { t, i18n } = useTranslation();

  return (
    <div role="listbox" aria-label={t("zdx.label.clipboard.list")} className="flex flex-col gap-2">
      {items.map((item) => {
        const active = item.id === activeId;
        return (
          <div
            id={`clip-${item.id}`}
            key={item.id}
            role="option"
            aria-selected={active}
            tabIndex={0}
            onClick={() => onActive(item.id)}
            onFocus={() => onActive(item.id)}
            className={active
              ? "flex items-start gap-3 rounded-xl border border-[#344c91] bg-[#eef2ff] p-3 dark:bg-[#243044]"
              : "flex items-start gap-3 rounded-xl border border-[#dfe5f0] bg-white p-3 dark:border-[#34415a] dark:bg-[#1b2537]"}
          >
            {item.kind === "image" ? <ClipThumb id={item.id} /> : <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg bg-[#e6ebff] text-[#4c5fda] dark:bg-[#243044]">{kindIcon(item.kind)}</div>}
            <div className="min-w-0 flex-1">
              <div className="truncate text-sm font-medium">{item.kind === "files" ? item.fileNames.join(", ") : item.preview}</div>
              <div className="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-xs text-[#667491] dark:text-[#aab7cf]">
                <span>{formatRelativeTime(item.lastCopiedAt, i18n.language)}</span>
                <span>{formatBytes(item.sizeBytes, i18n.language)}</span>
                <span>{t("zdx.text.clipboard.copiedTimes", { count: item.copyCount })}</span>
                {item.sourceApp ? <span>{t("zdx.text.clipboard.sourceApp", { app: item.sourceApp })}</span> : null}
              </div>
            </div>
            <div className="flex shrink-0 gap-1">
              <Button aria-label={item.pinned ? t("zdx.label.clipboard.unpin") : t("zdx.label.clipboard.pin")} type="text" icon={<PushpinOutlined />} onClick={() => onPin(item.id, !item.pinned)} />
              <Button aria-label={t("zdx.label.clipboard.copy")} type="text" icon={<CopyOutlined />} onClick={() => onCopy(item.id)} />
              <Button aria-label={t("zdx.label.clipboard.delete")} type="text" danger icon={<DeleteOutlined />} onClick={() => onDelete(item.id)} />
            </div>
          </div>
        );
      })}
    </div>
  );
}
