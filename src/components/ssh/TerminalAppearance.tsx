import { SettingGroup, SettingRow } from "@/components/SettingGroup";
import { useSshAppearanceStore } from "@/stores/sshAppearance";
import { isDarkTheme } from "@/utils/theme";
import { useSettingsStore } from "@/stores/settings";
import {
  resolvePalette,
  sshCursorStyles,
  sshFonts,
  sshThemes,
  terminalFontFamily,
  type SshCursorStyle,
  type SshFontId,
  type SshPalette,
  type SshPaletteKey,
  type SshThemeId,
} from "@/utils/sshAppearance";

const colorScheme = window.matchMedia("(prefers-color-scheme: dark)");
const subscribeScheme = (onChange: () => void) => {
  colorScheme.addEventListener("change", onChange);
  return () => colorScheme.removeEventListener("change", onChange);
};

const standardKeys = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"] as const;
const brightKeys = ["brightBlack", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightMagenta", "brightCyan", "brightWhite"] as const;
const faceKeys = ["background", "foreground", "cursor", "selection"] as const;

export default function TerminalAppearance({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useTranslation();
  const mode = useSettingsStore((state) => state.mode);
  const systemDark = useSyncExternalStore(subscribeScheme, () => colorScheme.matches);
  const dark = isDarkTheme(mode, systemDark);
  const appearance = useSshAppearanceStore();
  const palette = resolvePalette(appearance.themeId, appearance.custom, dark);
  const fontFamily = terminalFontFamily(appearance.fontId, appearance.customFont);

  function colorLabel(key: SshPaletteKey) {
    return t(`zdx.label.ssh.color${key[0].toUpperCase()}${key.slice(1)}`);
  }

  return (
    <Modal
      title={t("zdx.label.ssh.appearance")}
      open={open}
      width={640}
      centered
      styles={{
        body: {
          maxHeight: "calc(100vh - 180px)",
          overflowY: "auto",
        },
      }}
      onCancel={onClose}
      footer={[
        <Button key="reset" onClick={() => appearance.resetAppearance()}>{t("zdx.label.ssh.resetAppearance")}</Button>,
        <Button key="close" type="primary" onClick={onClose}>{t("zdx.label.ssh.confirm")}</Button>,
      ]}
    >
      <Flex vertical gap={16}>
        <div
          role="img"
          aria-label={t("zdx.label.ssh.preview")}
          className="rounded-md px-3 py-3"
          style={{ background: palette.background, color: palette.foreground, fontFamily, fontSize: appearance.fontSize, letterSpacing: appearance.letterSpacing }}
        >
          <span style={{ color: palette.green }}>Aa</span>{" "}
          <span style={{ color: palette.blue }}>Bb</span>{" "}
          <span style={{ color: palette.yellow }}>Cc</span>{" "}
          <span style={{ color: palette.red }}>Dd</span>{" "}
          <span style={{ color: palette.cyan }}>Ee</span>
        </div>
        <SettingGroup title={t("zdx.label.ssh.highlight")}>
          <SettingRow label={t("zdx.label.ssh.theme")} description={t("zdx.text.ssh.themeHint")}>
            <Select
              aria-label={t("zdx.label.ssh.theme")}
              className="w-40"
              value={appearance.themeId}
              onChange={(value) => appearance.setThemeId(value as SshThemeId)}
              options={sshThemes.map((theme) => ({ value: theme, label: t(`zdx.label.ssh.theme${theme[0].toUpperCase()}${theme.slice(1)}`) }))}
            />
          </SettingRow>
          <SettingRow label={t("zdx.label.ssh.contrast")} description={t("zdx.text.ssh.contrast")}>
            <Switch aria-label={t("zdx.label.ssh.contrast")} checked={appearance.contrast} onChange={appearance.setContrast} />
          </SettingRow>
          {appearance.themeId !== "custom" && (
            <SettingRow label={t("zdx.label.ssh.customizeTheme")}>
              <Button onClick={() => appearance.beginCustom(palette)}>{t("zdx.label.ssh.customizeTheme")}</Button>
            </SettingRow>
          )}
          {appearance.themeId === "custom" && (
            <SettingRow stacked label={t("zdx.label.ssh.themeCustom")}>
              <ColorGrid keys={faceKeys} palette={appearance.custom} label={colorLabel} onChange={appearance.setCustomColor} />
              <Typography.Text type="secondary">{t("zdx.label.ssh.colorStandard")}</Typography.Text>
              <ColorGrid keys={standardKeys} palette={appearance.custom} label={colorLabel} onChange={appearance.setCustomColor} />
              <Typography.Text type="secondary">{t("zdx.label.ssh.colorBright")}</Typography.Text>
              <ColorGrid keys={brightKeys} palette={appearance.custom} label={colorLabel} onChange={appearance.setCustomColor} />
            </SettingRow>
          )}
        </SettingGroup>
        <SettingGroup title={t("zdx.label.ssh.font")}>
          <SettingRow label={t("zdx.label.ssh.font")}>
            <Select
              aria-label={t("zdx.label.ssh.font")}
              className="w-44"
              value={appearance.fontId}
              onChange={(value) => appearance.setFontId(value as SshFontId)}
              options={sshFonts.map((font) => ({ value: font, label: t(`zdx.label.ssh.font${font[0].toUpperCase()}${font.slice(1)}`) }))}
            />
          </SettingRow>
          {appearance.fontId === "custom" && (
            <SettingRow label={t("zdx.label.ssh.fontCustom")}>
              <Input
                aria-label={t("zdx.label.ssh.fontCustom")}
                className="w-44"
                placeholder={t("zdx.placeholder.ssh.customFont")}
                value={appearance.customFont}
                onChange={(event) => appearance.setCustomFont(event.target.value)}
              />
            </SettingRow>
          )}
          <SettingRow label={t("zdx.label.ssh.fontSize")}>
            <InputNumber aria-label={t("zdx.label.ssh.fontSize")} min={11} max={24} value={appearance.fontSize} onChange={(value) => appearance.setFontSize(Number(value))} />
          </SettingRow>
          <SettingRow label={t("zdx.label.ssh.lineHeight")}>
            <InputNumber aria-label={t("zdx.label.ssh.lineHeight")} min={1} max={2} step={0.1} value={appearance.lineHeight} onChange={(value) => appearance.setLineHeight(Number(value))} />
          </SettingRow>
          <SettingRow label={t("zdx.label.ssh.letterSpacing")}>
            <InputNumber aria-label={t("zdx.label.ssh.letterSpacing")} min={0} max={6} value={appearance.letterSpacing} onChange={(value) => appearance.setLetterSpacing(Number(value))} />
          </SettingRow>
          <SettingRow label={t("zdx.label.ssh.cursor")}>
            <Select
              aria-label={t("zdx.label.ssh.cursor")}
              className="w-32"
              value={appearance.cursorStyle}
              onChange={(value) => appearance.setCursorStyle(value as SshCursorStyle)}
              options={sshCursorStyles.map((style) => ({ value: style, label: t(`zdx.label.ssh.cursor${style[0].toUpperCase()}${style.slice(1)}`) }))}
            />
          </SettingRow>
          <SettingRow label={t("zdx.label.ssh.cursorBlink")}>
            <Switch aria-label={t("zdx.label.ssh.cursorBlink")} checked={appearance.cursorBlink} onChange={appearance.setCursorBlink} />
          </SettingRow>
        </SettingGroup>
        <SettingGroup title={t("zdx.label.ssh.behavior")}>
          <SettingRow label={t("zdx.label.ssh.scrollback")} description={t("zdx.text.ssh.scrollback")}>
            <InputNumber aria-label={t("zdx.label.ssh.scrollback")} min={1000} max={100000} step={1000} value={appearance.scrollback} onChange={(value) => appearance.setScrollback(Number(value))} />
          </SettingRow>
          <SettingRow label={t("zdx.label.ssh.confirmPaste")} description={t("zdx.text.ssh.confirmPaste")}>
            <Switch aria-label={t("zdx.label.ssh.confirmPaste")} checked={appearance.confirmPaste} onChange={appearance.setConfirmPaste} />
          </SettingRow>
          <SettingRow label={t("zdx.label.ssh.copySkipsHistory")} description={t("zdx.text.ssh.copySkipsHistory")}>
            <Switch aria-label={t("zdx.label.ssh.copySkipsHistory")} checked={appearance.copySkipsHistory} onChange={appearance.setCopySkipsHistory} />
          </SettingRow>
        </SettingGroup>
      </Flex>
    </Modal>
  );
}

function ColorGrid({
  keys,
  palette,
  label,
  onChange,
}: {
  keys: readonly SshPaletteKey[];
  palette: SshPalette;
  label: (key: SshPaletteKey) => string;
  onChange: (key: SshPaletteKey, value: string) => void;
}) {
  return (
    <div className="grid grid-cols-2 gap-x-4 gap-y-2 sm:grid-cols-4">
      {keys.map((key) => (
        <label key={key} className="flex items-center gap-2">
          <input
            aria-label={label(key)}
            type="color"
            value={palette[key]}
            onChange={(event) => onChange(key, event.target.value)}
            className="h-7 w-9 cursor-pointer border-0 bg-transparent p-0"
          />
          <span className="text-xs">{label(key)}</span>
        </label>
      ))}
    </div>
  );
}
