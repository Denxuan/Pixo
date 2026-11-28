import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { ScreenshotRequestError, screenshotCopyText, screenshotLibrary, screenshotLibraryCopy, screenshotLibraryDelete, screenshotLibraryPath, screenshotLibraryPin, screenshotLibraryRecognize, screenshotLibraryThumb } from "@/api/screenshot";
import type { ShotFile } from "@/types/screenshot";
import { screenshotErrorText } from "@/utils/screenshotMessage";
import { inkFrameStyle } from "@/utils/theme";
import { formatBytes, formatRelativeTime } from "@/utils/time";

export default function ShotLibrary() {
  const { t, i18n } = useTranslation();
  const { token } = antdTheme.useToken();
  const [items, setItems] = useState<ShotFile[]>([]);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const [reading, setReading] = useState<number | null>(null);
  const [recognized, setRecognized] = useState("");

  const load = useCallback(async () => {
    try {
      setItems(await screenshotLibrary());
    } catch (caught) {
      if (caught instanceof ScreenshotRequestError) setError(screenshotErrorText(t, caught));
    }
  }, [t]);

  useEffect(() => {
    void load();
  }, [load]);

  async function copy(id: number) {
    setError("");
    try {
      await screenshotLibraryCopy(id);
      setNotice(t("zdx.text.clipboard.copied"));
    } catch (caught) {
      if (caught instanceof ScreenshotRequestError) setError(screenshotErrorText(t, caught));
    }
  }

  async function pin(id: number) {
    setError("");
    try {
      await screenshotLibraryPin(id);
      setNotice(t("zdx.text.screenshot.pinned"));
    } catch (caught) {
      if (caught instanceof ScreenshotRequestError) setError(screenshotErrorText(t, caught));
    }
  }

  async function recognize(id: number) {
    setError("");
    setReading(id);
    try {
      const result = await screenshotLibraryRecognize(id);
      setRecognized(result.text);
      setNotice(t("zdx.text.screenshot.ocrSaved"));
      await load();
    } catch (caught) {
      if (caught instanceof ScreenshotRequestError) setError(screenshotErrorText(t, caught));
    } finally {
      setReading(null);
    }
  }

  async function copyRecognized() {
    setError("");
    try {
      await screenshotCopyText(recognized);
      setNotice(t("zdx.text.screenshot.ocrCopied"));
    } catch (caught) {
      if (caught instanceof ScreenshotRequestError) setError(screenshotErrorText(t, caught));
    }
  }

  async function reveal(id: number) {
    setError("");
    try {
      await revealItemInDir(await screenshotLibraryPath(id));
    } catch (caught) {
      if (caught instanceof ScreenshotRequestError) setError(screenshotErrorText(t, caught));
      else setError(t("zdx.text.screenshot.error.storage"));
    }
  }

  async function remove(id: number) {
    setError("");
    try {
      await screenshotLibraryDelete(id);
      setNotice(t("zdx.text.screenshot.libraryRemoved"));
      await load();
    } catch (caught) {
      if (caught instanceof ScreenshotRequestError) setError(screenshotErrorText(t, caught));
    }
  }

  return (
    <Flex vertical gap={12}>
      <Typography.Text strong>{t("zdx.label.screenshot.library")}</Typography.Text>
      {error && <Alert type="error" showIcon message={error} />}
      {notice && <Alert type="success" showIcon closable message={notice} onClose={() => setNotice("")} />}
      {items.length === 0 && <Empty image={Empty.PRESENTED_IMAGE_SIMPLE} description={t("zdx.text.screenshot.libraryEmpty")} />}
      {items.length > 0 && (
        <div className="grid grid-cols-[repeat(auto-fill,minmax(220px,1fr))] gap-3">
          {items.map((item) => (
            <div key={item.id} className="flex flex-col overflow-hidden" style={{ background: token.colorBgContainer, ...inkFrameStyle(token, token.borderRadiusLG) }}>
              <ShotPreview item={item} />
              <div className="flex min-w-0 flex-1 flex-col gap-2 p-3">
                <Typography.Text className="truncate" title={item.text?.trim() || item.name}>{item.text?.trim() || item.name}</Typography.Text>
                <Typography.Text type="secondary" className="text-xs">
                  {item.kind === "video" ? t("zdx.label.screenshot.cardRecord") : t("zdx.label.screenshot.cardCapture")}
                  {" · "}
                  {formatBytes(item.bytes, i18n.language)}
                  {" · "}
                  {formatRelativeTime(item.createdAt, i18n.language)}
                </Typography.Text>
                <div className="mt-auto flex flex-wrap gap-1">
                  <Button size="small" type="link" onClick={() => void copy(item.id)}>{t("zdx.label.screenshot.copyShot")}</Button>
                  {item.kind === "image" && <Button size="small" type="link" loading={reading === item.id} disabled={reading !== null && reading !== item.id} onClick={() => void recognize(item.id)}>{t("zdx.label.screenshot.libraryOcr")}</Button>}
                  {item.kind === "image" && <Button size="small" type="link" onClick={() => void pin(item.id)}>{t("zdx.label.screenshot.pin")}</Button>}
                  <Button size="small" type="link" onClick={() => void reveal(item.id)}>{t("zdx.label.screenshot.reveal")}</Button>
                  <Popconfirm
                    title={t("zdx.label.screenshot.deleteShot")}
                    okText={t("zdx.label.clipboard.confirm")}
                    cancelText={t("zdx.label.clipboard.cancel")}
                    okButtonProps={{ danger: true }}
                    onConfirm={() => remove(item.id)}
                  >
                    <Button size="small" type="link" danger>{t("zdx.label.screenshot.deleteShot")}</Button>
                  </Popconfirm>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}
      <Modal
        title={t("zdx.label.screenshot.ocrResult")}
        open={recognized.length > 0}
        okText={t("zdx.label.screenshot.ocrCopy")}
        cancelText={t("zdx.label.screenshot.cancel")}
        onOk={() => void copyRecognized()}
        onCancel={() => setRecognized("")}
        destroyOnHidden
      >
        <Typography.Paragraph className="mb-0 max-h-80 overflow-auto whitespace-pre-wrap">{recognized}</Typography.Paragraph>
      </Modal>
    </Flex>
  );
}

function ShotPreview({ item }: { item: ShotFile }) {
  const { token } = antdTheme.useToken();
  const [url, setUrl] = useState("");

  useEffect(() => {
    if (item.kind === "video") return;
    let alive = true;
    let objectUrl = "";
    void screenshotLibraryThumb(item.id).then((bytes) => {
      if (!alive) return;
      const buffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
      objectUrl = URL.createObjectURL(new Blob([buffer], { type: "image/jpeg" }));
      setUrl(objectUrl);
    }).catch(() => undefined);
    return () => {
      alive = false;
      if (objectUrl) URL.revokeObjectURL(objectUrl);
    };
  }, [item.id, item.kind]);

  return (
    <div className="flex h-36 items-center justify-center overflow-hidden" style={{ background: token.colorFillQuaternary }}>
      {url ? <img src={url} alt="" className="h-full w-full object-cover" /> : item.kind === "video" ? <VideoCameraOutlined style={{ fontSize: 28, color: token.colorTextTertiary }} /> : <FileImageOutlined style={{ fontSize: 28, color: token.colorTextTertiary }} />}
    </div>
  );
}
