import { clipboardImage, clipboardSourceIcon } from "@/api/clipboard";
import type { ClipSummary } from "@/types/clipboard";
import { formatBytes, formatRelativeTime } from "@/utils/time";

type LoadedSource = { name: string; url: string };

const sourceCache = new Map<string, Promise<LoadedSource | null>>();

function loadSource(source: string): Promise<LoadedSource | null> {
  const cached = sourceCache.get(source);
  if (cached) return cached;
  const pending = clipboardSourceIcon(source)
    .then((mark) => {
      if (!mark) return null;
      if (mark.iconPng.length === 0) return { name: mark.name, url: "" };
      const buffer = mark.iconPng.buffer.slice(mark.iconPng.byteOffset, mark.iconPng.byteOffset + mark.iconPng.byteLength) as ArrayBuffer;
      return { name: mark.name, url: URL.createObjectURL(new Blob([buffer], { type: "image/png" })) };
    })
    .catch(() => null);
  sourceCache.set(source, pending);
  return pending;
}

function SourceApp({ source }: { source: string }) {
  const { t } = useTranslation();
  const [loaded, setLoaded] = useState<LoadedSource | null | undefined>(undefined);

  useEffect(() => {
    let alive = true;
    void loadSource(source).then((mark) => {
      if (alive) setLoaded(mark);
    });
    return () => {
      alive = false;
    };
  }, [source]);

  if (loaded === undefined) return null;
  const name = loaded?.name || source;
  return (
    <Tooltip title={t("zdx.text.clipboard.sourceApp", { app: name })}>
      <span role="img" aria-label={t("zdx.text.clipboard.sourceApp", { app: name })} className="inline-flex items-center justify-center w-5 h-5 shrink-0">
        {loaded?.url ? <img src={loaded.url} alt="" aria-hidden="true" width={18} height={18} style={{ borderRadius: 4 }} /> : <AppstoreOutlined aria-hidden="true" />}
      </span>
    </Tooltip>
  );
}

function MetaChip({ icon, text, label, shrink = false }: { icon?: ReactNode; text?: string; label: string; shrink?: boolean }) {
  return (
    <Tooltip title={label}>
      <span aria-label={label} className={shrink ? "inline-flex min-w-0 items-center gap-1" : "inline-flex shrink-0 items-center gap-1 whitespace-nowrap"}>
        {icon}
        {text ? <span className={shrink ? "truncate" : undefined}>{text}</span> : null}
      </span>
    </Tooltip>
  );
}

function kindIcon(kind: string) {
  if (kind === "image") return <FileImageOutlined />;
  if (kind === "files") return <FileOutlined />;
  return <FontSizeOutlined />;
}

function ClipAvatar({ id }: { id: number }) {
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

  return <Avatar shape="square" size={40} src={url || undefined} icon={kindIcon("image")} aria-label={t("zdx.label.clipboard.image")} />;
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
  const { token } = antdTheme.useToken();

  return (
    <List
      split={false}
      dataSource={items}
      rowKey="id"
      aria-label={t("zdx.label.clipboard.list")}
      renderItem={(item) => {
        const active = item.id === activeId;
        const title = item.kind === "files" ? item.fileNames.join(", ") : item.preview;
        const copiedAt = formatRelativeTime(item.lastCopiedAt, i18n.language);
        const size = formatBytes(item.sizeBytes, i18n.language);
        const copied = t("zdx.text.clipboard.copiedTimes", { count: item.copyCount });
        return (
          <List.Item
            id={`clip-${item.id}`}
            aria-selected={active}
            tabIndex={0}
            onClick={() => onActive(item.id)}
            onFocus={(event) => {
              if (event.target instanceof Element && event.target.closest("button")) return;
              onActive(item.id);
            }}
            style={{
              display: "block",
              background: active ? token.colorPrimaryBg : item.pinned ? token.colorWarningBg : token.colorBgContainer,
              border: `1px solid ${active ? token.colorPrimaryBorder : item.pinned ? token.colorWarningBorder : token.colorBorderSecondary}`,
              boxShadow: item.pinned ? `inset 3px 0 0 ${token.colorWarning}` : undefined,
              borderRadius: token.borderRadiusLG,
              marginBottom: 6,
              padding: "8px 8px 8px 10px",
              cursor: "pointer",
            }}
          >
            <div className="flex items-center gap-2">
              <div className="shrink-0">
                {item.kind === "image" ? <ClipAvatar id={item.id} /> : <Avatar shape="square" size={40} icon={kindIcon(item.kind)} />}
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-1">
                  <Typography.Text ellipsis style={{ flex: 1, minWidth: 0, margin: 0 }}>{title}</Typography.Text>
                  <div className="flex shrink-0 items-center">
                    <Button type="text" size="small" aria-label={item.pinned ? t("zdx.label.clipboard.unpin") : t("zdx.label.clipboard.pin")} aria-pressed={item.pinned} icon={item.pinned ? <PushpinFilled /> : <PushpinOutlined />} style={item.pinned ? { color: token.colorWarning } : undefined} onClick={(event) => { event.stopPropagation(); onPin(item.id, !item.pinned); }} />
                    <Button type="text" size="small" aria-label={t("zdx.label.clipboard.copy")} icon={<CopyOutlined />} onClick={(event) => { event.stopPropagation(); onCopy(item.id); }} />
                    <Button type="text" size="small" danger aria-label={t("zdx.label.clipboard.delete")} icon={<DeleteOutlined />} onClick={(event) => { event.stopPropagation(); onDelete(item.id); }} />
                  </div>
                </div>
                <div className="mt-1 flex min-w-0 items-center gap-3" style={{ color: token.colorTextSecondary, fontSize: token.fontSizeSM }}>
                  <MetaChip shrink icon={<ClockCircleOutlined />} text={copiedAt} label={copiedAt} />
                  <MetaChip text={size} label={size} />
                  <MetaChip icon={<HistoryOutlined />} text={String(item.copyCount)} label={copied} />
                  {item.formatted ? <MetaChip icon={<FontColorsOutlined />} label={t("zdx.label.clipboard.formatted")} /> : null}
                  {item.sourceApp ? <SourceApp source={item.sourceApp} /> : null}
                </div>
              </div>
            </div>
          </List.Item>
        );
      }}
    />
  );
}
