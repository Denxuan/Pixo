import { formatShortcut, shortcutFromEvent } from "@/utils/shortcut";

export default function ShortcutField({
  value,
  macos,
  label,
  onChange,
}: {
  value: string;
  macos: boolean;
  label?: string;
  onChange: (shortcut: string) => void;
}) {
  const { t } = useTranslation();
  const [recording, setRecording] = useState(false);
  const [needsModifier, setNeedsModifier] = useState(false);

  function onKeyDown(event: { key: string; code: string; metaKey: boolean; ctrlKey: boolean; altKey: boolean; shiftKey: boolean; preventDefault: () => void; stopPropagation: () => void }) {
    if (!recording) return;
    event.preventDefault();
    event.stopPropagation();
    const bareEscape = event.key === "Escape" && !event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey;
    if (bareEscape) {
      setRecording(false);
      setNeedsModifier(false);
      return;
    }
    const shortcut = shortcutFromEvent(event);
    if (!shortcut) {
      if (event.key !== "Meta" && event.key !== "Control" && event.key !== "Alt" && event.key !== "Shift") {
        setNeedsModifier(true);
      }
      return;
    }
    onChange(shortcut);
    setNeedsModifier(false);
    setRecording(false);
  }

  return (
    <Flex vertical gap={4}>
      <Input
        readOnly
        value={recording ? t("zdx.text.clipboard.shortcutRecording") : formatShortcut(value, macos)}
        aria-label={label ?? t("zdx.label.clipboard.shortcut")}
        onFocus={() => {
          setRecording(true);
          setNeedsModifier(false);
        }}
        onBlur={() => setRecording(false)}
        onKeyDown={onKeyDown}
      />
      {needsModifier ? <Typography.Text type="secondary">{t("zdx.text.clipboard.shortcutNeedModifier")}</Typography.Text> : null}
    </Flex>
  );
}
