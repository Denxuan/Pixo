import { clipboardGet, clipboardImage, clipboardSourceIcon } from "@/api/clipboard";
import Kbd from "@/components/Kbd";
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
    if (source === "screenshot" || source === "Pixo") {
      setLoaded({ name: source === "screenshot" ? t("zdx.label.clipboard.sourceScreenshot") : "Pixo", url: "" });
      return;
    }
    let alive = true;
    void loadSource(source).then((mark) => {
      if (alive) setLoaded(mark);
    });
    return () => {
      alive = false;
    };
  }, [source, t]);

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

  return <KindBadge kind="image" src={url} label={t("zdx.label.clipboard.image")} />;
}

function KindBadge({ kind, src, label }: { kind: string; src?: string; label?: string }) {
  const { token } = antdTheme.useToken();
  return (
    <Avatar
      shape="square"
      size={34}
      src={src || undefined}
      icon={kindIcon(kind)}
      aria-label={label}
      style={{ borderRadius: token.borderRadius, background: src ? token.colorFillQuaternary : token.colorFillSecondary, color: token.colorTextSecondary, fontSize: 16 }}
    />
  );
}

const PREVIEW_LIMIT = 8000;

const previewStyles = {
  root: { maxWidth: "min(308px, calc(100vw - 24px))" },
  container: { maxWidth: "min(308px, calc(100vw - 24px))", overflow: "hidden" },
};

const previewScroll = {
  maxHeight: "min(220px, calc(100vh - 96px))",
  overflow: "auto",
  overflowWrap: "anywhere" as const,
  whiteSpace: "pre-wrap" as const,
  overscrollBehavior: "contain" as const,
};

function ClipTitle({ item }: { item: ClipSummary }) {
  const { t } = useTranslation();
  const [full, setFull] = useState("");
  const [truncated, setTruncated] = useState(false);
  const [previewUrl, setPreviewUrl] = useState("");
  const content = item.kind === "files" ? item.fileNames.join(", ") : item.preview || t("zdx.label.clipboard.image");
  const named = item.label?.trim() ?? "";
  const title = named || content;

  useEffect(() => {
    return () => {
      if (previewUrl) URL.revokeObjectURL(previewUrl);
    };
  }, [previewUrl]);

  function load() {
    if (item.kind === "image") {
      if (previewUrl) return;
      void clipboardImage(item.id, true).then((bytes) => {
        const buffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
        setPreviewUrl(URL.createObjectURL(new Blob([buffer], { type: "image/png" })));
      }).catch(() => undefined);
      return;
    }
    if (full) return;
    void clipboardGet(item.id).then((detail) => {
      const text = detail.text ?? (detail.files.length > 0 ? detail.files.join("\n") : title);
      setTruncated(text.length > PREVIEW_LIMIT);
      setFull(text.slice(0, PREVIEW_LIMIT));
    }).catch(() => setFull(title));
  }

  const previewLabel = t("zdx.label.clipboard.preview");
  const preview = item.kind === "image" ? (
    <div>
      {previewUrl ? (
        <img src={previewUrl} alt={previewLabel} style={{ display: "block", maxWidth: "100%", maxHeight: 180, objectFit: "contain" }} />
      ) : (
        <span>{t("zdx.label.clipboard.image")}</span>
      )}
      {item.imageWidth && item.imageHeight ? (
        <div style={{ marginTop: 6 }}>{t("zdx.text.clipboard.imageSize", { width: item.imageWidth, height: item.imageHeight })}</div>
      ) : null}
    </div>
  ) : (
    <div>
      {full || title}
      {truncated ? <div style={{ marginTop: 6 }}>{t("zdx.text.clipboard.previewTruncated")}</div> : null}
    </div>
  );

  return (
    <Popover mouseEnterDelay={0.35} placement="topLeft" destroyOnHidden styles={previewStyles} content={<div role="region" aria-label={previewLabel} style={previewScroll}>{preview}</div>}>
      <span className="block min-w-0 flex-1 overflow-hidden" onMouseEnter={load}>
        <Typography.Text ellipsis style={{ display: "block", margin: 0, maxWidth: "100%" }}>{title}</Typography.Text>
        {named && <Typography.Text ellipsis type="secondary" style={{ display: "block", margin: 0, maxWidth: "100%", fontSize: 12 }}>{content}</Typography.Text>}
      </span>
    </Popover>
  );
}

export default function HistoryList({
  items,
  activeId,
  onActive,
  onCopy,
  onPin,
  onStick,
  stickingId,
  onDelete,
  numberShortcuts,
}: {
  items: ClipSummary[];
  activeId: number | null;
  onActive: (id: number, pointer?: { altKey: boolean; shiftKey: boolean }) => void;
  onCopy: (id: number) => void;
  onPin: (id: number, pinned: boolean) => void;
  onStick?: (id: number) => void;
  stickingId?: number | null;
  onDelete: (id: number) => void;
  numberShortcuts?: Array<{ copy: string; paste: string }>;
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
        const rowNumber = numberShortcuts?.[items.findIndex((entry) => entry.id === item.id)];
        const copiedAt = formatRelativeTime(item.lastCopiedAt, i18n.language);
        const size = formatBytes(item.sizeBytes, i18n.language);
        const copied = t("zdx.text.clipboard.copiedTimes", { count: item.copyCount });
        return (
          <List.Item
            id={`clip-${item.id}`}
            aria-selected={active}
            tabIndex={0}
            onMouseDown={(event) => {
              if (!event.altKey || (event.target instanceof Element && event.target.closest("button"))) return;
              event.preventDefault();
              onActive(item.id, event);
            }}
            onClick={(event) => {
              if (event.altKey || (event.target instanceof Element && event.target.closest("button"))) return;
              onActive(item.id, event);
            }}
            onFocus={(event) => {
              if (event.target instanceof Element && event.target.closest("button")) return;
              onActive(item.id);
            }}
            className={active ? "group outline-none" : "group outline-none hover:bg-black/[0.035] focus-visible:ring-2 dark:hover:bg-white/[0.05]"}
            style={{
              display: "block",
              background: active ? token.colorPrimaryBg : undefined,
              border: `1px solid ${active ? token.colorPrimary : "transparent"}`,
              borderRadius: token.borderRadius,
              marginBottom: 8,
              padding: "8px 10px",
              cursor: "pointer",
            }}
          >
            <div className="flex items-center gap-3">
              <div className="shrink-0">
                {item.kind === "image" ? <ClipAvatar id={item.id} /> : <KindBadge kind={item.kind} />}
              </div>
              <div className="min-w-0 flex-1">
                <div className="flex min-w-0 items-center gap-1.5">
                  {item.pinned ? <PushpinFilled aria-hidden="true" style={{ color: token.colorWarning, fontSize: 12 }} /> : null}
                  <ClipTitle item={item} />
                </div>
                <div className="mt-0.5 flex min-w-0 items-center gap-2.5" style={{ color: token.colorTextTertiary, fontSize: token.fontSizeSM }}>
                  <MetaChip shrink icon={<ClockCircleOutlined />} text={copiedAt} label={copiedAt} />
                  <MetaChip text={size} label={size} />
                  <MetaChip icon={<HistoryOutlined />} text={String(item.copyCount)} label={copied} />
                  {item.formatted ? <MetaChip icon={<FontColorsOutlined />} label={t("zdx.label.clipboard.formatted")} /> : null}
                  {item.sourceApp ? <SourceApp source={item.sourceApp} /> : null}
                </div>
              </div>
              <div className="flex shrink-0 items-center gap-1.5">
                <div className={active ? "flex items-center" : "flex items-center opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100"}>
                  <Button type="text" size="small" aria-label={item.pinned ? t("zdx.label.clipboard.unpin") : t("zdx.label.clipboard.pin")} aria-pressed={item.pinned} icon={item.pinned ? <PushpinFilled /> : <PushpinOutlined />} style={item.pinned ? { color: token.colorWarning } : { color: token.colorTextSecondary }} onClick={(event) => { event.stopPropagation(); onPin(item.id, !item.pinned); }} />
                  {item.kind === "image" && onStick && <Button type="text" size="small" loading={stickingId === item.id} aria-label={t("zdx.label.clipboard.stick")} icon={<BorderOuterOutlined />} style={{ color: token.colorTextSecondary }} onClick={(event) => { event.stopPropagation(); onStick(item.id); }} />}
                  <Button type="text" size="small" aria-label={t("zdx.label.clipboard.copy")} icon={<CopyOutlined />} style={{ color: token.colorTextSecondary }} onClick={(event) => { event.stopPropagation(); onCopy(item.id); }} />
                  <Button type="text" size="small" danger aria-label={t("zdx.label.clipboard.delete")} icon={<DeleteOutlined />} onClick={(event) => { event.stopPropagation(); onDelete(item.id); }} />
                </div>
                {item.pinKey ? <Kbd label={t("zdx.label.clipboard.pinKey", { key: item.pinKey })}>{item.pinKey.toUpperCase()}</Kbd> : null}
                {rowNumber ? (
                  <Tooltip title={t("zdx.text.clipboard.hintRow", rowNumber)}>
                    <span className="inline-flex"><Kbd label={t("zdx.text.clipboard.hintRow", rowNumber)}>{rowNumber.copy}</Kbd></span>
                  </Tooltip>
                ) : null}
              </div>
            </div>
          </List.Item>
        );
      }}
    />
  );
}
