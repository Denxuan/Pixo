import { useSettingsStore, type ThemeMode } from "@/stores/settings";

function navClass(active: boolean) {
  if (active) return "flex items-center gap-3 rounded-lg bg-[#344c91] px-3 py-3 text-sm font-medium text-white";
  return "flex items-center gap-3 rounded-lg px-3 py-3 text-sm font-medium text-[#d5def3] hover:bg-[#1c2a4d]";
}

export default function AppShell() {
  const { t } = useTranslation();
  const mode = useSettingsStore((state) => state.mode);
  const setMode = useSettingsStore((state) => state.setMode);
  const language = useSettingsStore((state) => state.language);
  const setLanguage = useSettingsStore((state) => state.setLanguage);

  return (
    <div className="flex h-full overflow-hidden bg-[#f5f7fb] font-sans text-[#182340] dark:bg-[#111827] dark:text-[#e7ecf7]">
      <aside className="flex w-58 shrink-0 flex-col bg-[#101a38] px-5 py-6 text-white">
        <div className="flex items-center gap-3 px-1">
          <img src="/pixo-icon.png" alt="" className="h-10 w-10 rounded-xl" />
          <div>
            <div className="text-xl font-bold tracking-tight">Pixo</div>
            <div className="text-xs text-[#aab7d3]">{t("zdx.text.app.tagline")}</div>
          </div>
        </div>
        <div className="mt-12 mb-3 px-2 text-xs font-medium text-[#92a2c6]">{t("zdx.label.nav.workspace")}</div>
        <nav className="flex flex-col gap-2" aria-label={t("zdx.label.nav.workspace")}>
          <NavLink to="/json" className={({ isActive }) => navClass(isActive)}><CodeOutlined aria-hidden="true" />{t("zdx.label.json.title")}</NavLink>
          <NavLink to="/clipboard" className={({ isActive }) => navClass(isActive)}><HistoryOutlined aria-hidden="true" />{t("zdx.label.nav.clipboard")}</NavLink>
        </nav>
        <div className="mt-auto px-2 pt-10 text-xs text-[#aab7d3]">{t("zdx.text.app.local")}</div>
      </aside>
      <main className="flex min-w-0 flex-1 flex-col overflow-hidden px-5 py-5 md:px-10">
        <header className="flex shrink-0 items-start justify-between gap-4 border-b border-[#dfe5f0] pb-5 dark:border-[#34415a]">
          <p className="m-0 text-sm text-[#667491] dark:text-[#aab7cf]">{t("zdx.text.app.welcome")}</p>
          <div className="flex flex-wrap items-center justify-end gap-2">
            <label htmlFor="theme-mode" className="sr-only">{t("zdx.label.theme.title")}</label>
            <select id="theme-mode" value={mode} onChange={(event) => setMode(event.target.value as ThemeMode)} className="cursor-pointer rounded-lg border border-[#d6deeb] bg-white px-3 py-2 text-sm text-[#405170] focus-visible:outline-2 focus-visible:outline-[#8193ff] dark:border-[#46546b] dark:bg-[#202b40] dark:text-[#e7ecf7]">
              <option value="system">{t("zdx.label.theme.system")}</option>
              <option value="light">{t("zdx.label.theme.light")}</option>
              <option value="dark">{t("zdx.label.theme.dark")}</option>
            </select>
            <button type="button" onClick={() => setLanguage(language === "en" ? "zh-CN" : "en")} className="cursor-pointer rounded-lg border border-[#d6deeb] bg-white px-3 py-2 text-sm text-[#405170] hover:bg-[#eef2fa] focus-visible:outline-2 focus-visible:outline-[#536be2] dark:border-[#46546b] dark:bg-[#202b40] dark:text-[#e7ecf7] dark:hover:bg-[#2d3b55]" aria-label={t("zdx.label.language.switch")}>
              <GlobalOutlined aria-hidden="true" /> <span className="ml-1">{t("zdx.label.language.toggle")}</span>
            </button>
          </div>
        </header>
        <div className="flex min-h-0 flex-1 flex-col">
          <Outlet />
        </div>
      </main>
    </div>
  );
}
