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
    <span className="inline-flex items-center gap-1">
      {loaded?.url ? <img src={loaded.url} alt="" aria-hidden="true" width={16} height={16} style={{ borderRadius: 4 }} /> : null}
      <span>{t("zdx.text.clipboard.sourceApp", { app: name })}</span>
    </span>
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

  return <Avatar shape="square" size={48} src={url || undefined} icon={kindIcon("image")} aria-label={t("zdx.label.clipboard.image")} />;
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
      dataSource={items}
      rowKey="id"
      aria-label={t("zdx.label.clipboard.list")}
      renderItem={(item) => {
        const active = item.id === activeId;
        const title = item.kind === "files" ? item.fileNames.join(", ") : item.preview;
        return (
          <List.Item
            id={`clip-${item.id}`}
            aria-selected={active}
            tabIndex={0}
            onClick={() => onActive(item.id)}
            onFocus={() => onActive(item.id)}
            style={{
              background: active ? token.colorPrimaryBg : item.pinned ? token.colorWarningBg : token.colorBgContainer,
              border: `1px solid ${active ? token.colorPrimaryBorder : item.pinned ? token.colorWarningBorder : token.colorBorderSecondary}`,
              boxShadow: item.pinned ? `inset 3px 0 0 ${token.colorWarning}` : undefined,
              borderRadius: token.borderRadiusLG,
              marginBottom: 8,
              paddingInline: 12,
              cursor: "pointer",
            }}
            actions={[
              <Button key="pin" type="text" aria-label={item.pinned ? t("zdx.label.clipboard.unpin") : t("zdx.label.clipboard.pin")} aria-pressed={item.pinned} icon={item.pinned ? <PushpinFilled /> : <PushpinOutlined />} style={item.pinned ? { color: token.colorWarning } : undefined} onClick={(event) => { event.stopPropagation(); onPin(item.id, !item.pinned); }} />,
              <Button key="copy" type="text" aria-label={t("zdx.label.clipboard.copy")} icon={<CopyOutlined />} onClick={(event) => { event.stopPropagation(); onCopy(item.id); }} />,
              <Button key="delete" type="text" danger aria-label={t("zdx.label.clipboard.delete")} icon={<DeleteOutlined />} onClick={(event) => { event.stopPropagation(); onDelete(item.id); }} />,
            ]}
          >
            <List.Item.Meta
              avatar={item.kind === "image" ? <ClipAvatar id={item.id} /> : <Avatar shape="square" size={48} icon={kindIcon(item.kind)} />}
              title={<Typography.Text ellipsis>{title}</Typography.Text>}
              description={
                <Space size={12} wrap>
                  <span>{formatRelativeTime(item.lastCopiedAt, i18n.language)}</span>
                  <span>{formatBytes(item.sizeBytes, i18n.language)}</span>
                  <span>{t("zdx.text.clipboard.copiedTimes", { count: item.copyCount })}</span>
                  {item.sourceApp ? <SourceApp source={item.sourceApp} /> : null}
                </Space>
              }
            />
          </List.Item>
        );
      }}
    />
  );
}
