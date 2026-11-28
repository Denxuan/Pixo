import { dockerSetImageUpdates, dockerStatus } from "@/api/docker";
import { SettingGroup, SettingRow } from "@/components/SettingGroup";
import { isDesktop } from "@/utils/desktop";

export default function DockerSettingsPage() {
  const { t } = useTranslation();
  const desktop = isDesktop();
  const [imageUpdates, setImageUpdates] = useState<boolean | null>(desktop ? null : false);

  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    void dockerStatus()
      .then((status) => {
        if (alive) setImageUpdates(status.imageUpdates);
      })
      .catch(() => {
        if (alive) setImageUpdates(false);
      });
    return () => {
      alive = false;
    };
  }, [desktop]);

  function changeImageUpdates(enabled: boolean) {
    const previous = imageUpdates;
    setImageUpdates(enabled);
    void dockerSetImageUpdates(enabled).catch(() => setImageUpdates(previous));
  }

  return (
    <Flex vertical gap={24}>
      {!desktop && <Alert type="info" showIcon message={t("zdx.text.docker.desktopOnly")} />}
      <SettingGroup title={t("zdx.label.docker.kind")}>
        <SettingRow label={t("zdx.label.docker.kindLocal")} description={t("zdx.text.docker.kindLocalHelp")} />
        <SettingRow label={t("zdx.label.docker.kindSsh")} description={t("zdx.text.docker.kindSshHelp")} />
      </SettingGroup>
      <SettingGroup title={t("zdx.label.docker.images")}>
        <SettingRow label={t("zdx.label.docker.checkUpdates")} description={t("zdx.text.docker.imageUpdatesHelp")}>
          <Switch
            aria-label={t("zdx.label.docker.checkUpdates")}
            checked={imageUpdates === true}
            disabled={!desktop || imageUpdates === null}
            onChange={changeImageUpdates}
          />
        </SettingRow>
      </SettingGroup>
      <SettingGroup title={t("zdx.label.docker.pull")}>
        <SettingRow label={t("zdx.label.docker.rememberRegistry")} description={t("zdx.text.docker.registryHint")} />
      </SettingGroup>
    </Flex>
  );
}
