import { clipboardImage } from "@/api/clipboard";
import type { ClipSummary } from "@/types/clipboard";
import { formatBytes, formatRelativeTime } from "@/utils/time";

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
              background: active ? token.colorPrimaryBg : token.colorBgContainer,
              border: `1px solid ${active ? token.colorPrimaryBorder : token.colorBorderSecondary}`,
              borderRadius: token.borderRadiusLG,
              marginBottom: 8,
              paddingInline: 12,
              cursor: "pointer",
            }}
            actions={[
              <Button key="pin" type="text" aria-label={item.pinned ? t("zdx.label.clipboard.unpin") : t("zdx.label.clipboard.pin")} icon={<PushpinOutlined />} onClick={(event) => { event.stopPropagation(); onPin(item.id, !item.pinned); }} />,
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
                  {item.sourceApp ? <span>{t("zdx.text.clipboard.sourceApp", { app: item.sourceApp })}</span> : null}
                </Space>
              }
            />
          </List.Item>
        );
      }}
    />
  );
}
