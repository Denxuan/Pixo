import { SettingGroup, SettingRow } from "@/components/SettingGroup";
import { isDesktop } from "@/utils/desktop";

export default function DockerSettingsPage() {
  const { t } = useTranslation();
  const desktop = isDesktop();

  return (
    <Flex vertical gap={24}>
      {!desktop && <Alert type="info" showIcon message={t("zdx.text.docker.desktopOnly")} />}
      <Typography.Paragraph type="secondary">{t("zdx.text.docker.settingsDescription")}</Typography.Paragraph>
      <SettingGroup title={t("zdx.label.docker.kind")}>
        <SettingRow label={t("zdx.label.docker.kindLocal")} description={t("zdx.text.docker.kindLocalHelp")} />
        <SettingRow label={t("zdx.label.docker.kindSsh")} description={t("zdx.text.docker.kindSshHelp")} />
      </SettingGroup>
      <SettingGroup title={t("zdx.label.docker.pull")}>
        <SettingRow label={t("zdx.label.docker.rememberRegistry")} description={t("zdx.text.docker.registryHint")} />
      </SettingGroup>
    </Flex>
  );
}
