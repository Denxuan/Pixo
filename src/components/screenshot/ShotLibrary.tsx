import { revealItemInDir } from "@tauri-apps/plugin-opener";

import { ScreenshotRequestError, screenshotLibrary, screenshotLibraryCopy, screenshotLibraryDelete, screenshotLibraryPath } from "@/api/screenshot";
import type { ShotFile } from "@/types/screenshot";
import { screenshotErrorText } from "@/utils/screenshotMessage";
import { formatBytes, formatRelativeTime } from "@/utils/time";

export default function ShotLibrary() {
  const { t, i18n } = useTranslation();
  const { token } = antdTheme.useToken();
  const [items, setItems] = useState<ShotFile[]>([]);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");

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
      setNotice(t("zdx.text.clipboard.transformed"));
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
        <List
          dataSource={items}
          rowKey="id"
          renderItem={(item) => (
            <List.Item
              actions={[
                <Button key="copy" type="link" onClick={() => void copy(item.id)}>{t("zdx.label.screenshot.copyShot")}</Button>,
                <Button key="reveal" type="link" onClick={() => void reveal(item.id)}>{t("zdx.label.screenshot.reveal")}</Button>,
                <Popconfirm
                  key="delete"
                  title={t("zdx.label.screenshot.deleteShot")}
                  okText={t("zdx.label.clipboard.confirm")}
                  cancelText={t("zdx.label.clipboard.cancel")}
                  okButtonProps={{ danger: true }}
                  onConfirm={() => remove(item.id)}
                >
                  <Button type="link" danger>{t("zdx.label.screenshot.deleteShot")}</Button>
                </Popconfirm>,
              ]}
            >
              <List.Item.Meta
                title={<span className="truncate">{item.text?.trim() || item.name}</span>}
                description={
                  <span style={{ color: token.colorTextTertiary }}>
                    {item.kind === "video" ? t("zdx.label.screenshot.cardRecord") : t("zdx.label.screenshot.cardCapture")}
                    {" · "}
                    {formatBytes(item.bytes, i18n.language)}
                    {" · "}
                    {formatRelativeTime(item.createdAt, i18n.language)}
                  </span>
                }
              />
            </List.Item>
          )}
        />
      )}
    </Flex>
  );
}
