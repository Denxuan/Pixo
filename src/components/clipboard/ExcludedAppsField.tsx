import { clipboardInstalledApps, clipboardSourceIcon, toClipboardError } from "@/api/clipboard";
import { clipboardErrorText } from "@/utils/clipboardMessage";

const limit = 100;
const iconLoads = 4;

type ListedApp = { id: string; name: string; icon: string };

function pngUrl(bytes: Uint8Array): string {
  if (bytes.length === 0) return "";
  const buffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
  return URL.createObjectURL(new Blob([buffer], { type: "image/png" }));
}

function AppChoice({ icon, name }: { icon: string; name: string }) {
  return (
    <span className="inline-flex items-center gap-2">
      {icon ? <img src={icon} alt="" width={16} height={16} style={{ borderRadius: 4 }} /> : <span className="inline-block h-4 w-4" aria-hidden="true" />}
      <span>{name}</span>
    </span>
  );
}

async function loadMissingIcons(apps: ListedApp[], alive: () => boolean, keep: (url: string) => void, update: (id: string, icon: string) => void) {
  const missing = apps.filter((app) => !app.icon);
  let cursor = 0;
  async function worker() {
    while (alive()) {
      const index = cursor;
      cursor += 1;
      if (index >= missing.length) return;
      const app = missing[index];
      const mark = await clipboardSourceIcon(app.id).catch(() => null);
      if (!alive() || !mark || mark.iconPng.length === 0) continue;
      const icon = pngUrl(mark.iconPng);
      if (!icon) continue;
      keep(icon);
      update(app.id, icon);
    }
  }
  await Promise.all(Array.from({ length: Math.min(iconLoads, missing.length) }, () => worker()));
}

export default function ExcludedAppsField({ value, onChange }: { value: string[]; onChange: (ids: string[]) => void }) {
  const { t } = useTranslation();
  const [apps, setApps] = useState<ListedApp[]>([]);
  const [extra, setExtra] = useState<Record<string, ListedApp>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const requested = useRef(new Set<string>());
  const urls = useRef<string[]>([]);
  const translate = useRef(t);
  translate.current = t;

  useEffect(() => {
    const owned = urls.current;
    return () => {
      for (const url of owned) URL.revokeObjectURL(url);
    };
  }, []);

  useEffect(() => {
    let alive = true;
    void clipboardInstalledApps()
      .then(async (list) => {
        const listed = list.map((app) => {
          const icon = pngUrl(app.iconPng);
          if (icon) urls.current.push(icon);
          return { id: app.id, name: app.name, icon };
        });
        if (alive) setApps(listed);
        await loadMissingIcons(
          listed,
          () => alive,
          (url) => urls.current.push(url),
          (id, icon) => {
            if (alive) setApps((current) => current.map((app) => (app.id === id ? { ...app, icon } : app)));
          },
        );
      })
      .catch((caught) => {
        if (alive) setError(clipboardErrorText(translate.current, toClipboardError(caught)));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, []);

  const catalogKey = apps.map((app) => app.id).join("\n");
  useEffect(() => {
    if (loading) return;
    const known = new Set(catalogKey.split("\n").filter((id) => id.length > 0));
    const missing = value.filter((id) => !known.has(id) && !requested.current.has(id));
    if (missing.length === 0) return;
    let alive = true;
    for (const id of missing) {
      requested.current.add(id);
      void clipboardSourceIcon(id)
        .then((mark) => {
          if (!alive || !mark?.name) return;
          const icon = pngUrl(mark.iconPng);
          if (icon) urls.current.push(icon);
          setExtra((current) => ({ ...current, [id]: { id, name: mark.name, icon } }));
        })
        .catch(() => undefined);
    }
    return () => {
      alive = false;
    };
  }, [catalogKey, loading, value]);

  const icons = new Map<string, string>();
  const options = apps.map((app) => {
    if (app.icon) icons.set(app.id, app.icon);
    return { value: app.id, label: app.name };
  });
  for (const id of value) {
    const named = extra[id];
    if (named?.icon) icons.set(id, named.icon);
    if (!options.some((option) => option.value === id)) {
      options.push({ value: id, label: named?.name ?? id });
    }
  }
  options.sort((left, right) => left.label.localeCompare(right.label, undefined, { sensitivity: "base" }));

  return (
    <Flex vertical gap={8}>
      <Select
        mode="multiple"
        showSearch
        allowClear
        optionFilterProp="label"
        loading={loading}
        value={value}
        options={options}
        aria-label={t("zdx.label.clipboard.excludedApps")}
        placeholder={t("zdx.placeholder.clipboard.excludedSearch")}
        style={{ width: "100%" }}
        listHeight={320}
        notFoundContent={loading ? <Spin size="small" /> : t("zdx.text.clipboard.excludedNoMatch")}
        optionRender={(option) => <AppChoice icon={icons.get(String(option.value)) ?? ""} name={String(option.label ?? "")} />}
        labelRender={(option) => <AppChoice icon={icons.get(String(option.value)) ?? ""} name={String(option.label ?? option.value ?? "")} />}
        onChange={(ids) => onChange(ids.slice(0, limit))}
      />
      {value.length >= limit && <Typography.Text type="secondary">{t("zdx.text.clipboard.excludedLimit")}</Typography.Text>}
      {error && <Typography.Text type="danger">{error}</Typography.Text>}
    </Flex>
  );
}
