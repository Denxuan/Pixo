import { formatJson } from "@/utils/json";
import { useSettingsStore, type ThemeMode } from "@/stores/settings";

export default function Workspace() {
  const { t } = useTranslation();
  const [input, setInput] = useState("");
  const [output, setOutput] = useState("");
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);
  const mode = useSettingsStore((state) => state.mode);
  const setMode = useSettingsStore((state) => state.setMode);
  const language = useSettingsStore((state) => state.language);
  const setLanguage = useSettingsStore((state) => state.setLanguage);

  function transform(compact: boolean) {
    setCopied(false);
    try {
      setOutput(formatJson(input, compact));
      setError("");
    } catch {
      setOutput("");
      setError("zdx.text.json.invalid");
    }
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(output);
      setCopied(true);
      setError("");
    } catch {
      setError("zdx.text.json.copyError");
    }
  }

  return (
    <div className="min-h-screen bg-[#f5f7fb] text-[#182340] dark:bg-[#111827] dark:text-[#e7ecf7] font-sans md:flex">
      <aside className="w-full md:w-58 md:min-h-screen shrink-0 bg-[#101a38] text-white px-5 py-6 flex flex-col">
        <div className="flex items-center gap-3 px-1">
          <img src="/pixo-icon.png" alt="" className="w-10 h-10 rounded-xl" />
          <div><div className="text-xl font-bold tracking-tight">Pixo</div><div className="text-xs text-[#aab7d3]">{t("zdx.text.app.tagline")}</div></div>
        </div>
        <div className="mt-12 mb-3 px-2 text-xs font-medium text-[#92a2c6]">{t("zdx.label.nav.workspace")}</div>
        <div className="flex items-center gap-3 rounded-lg bg-[#344c91] px-3 py-3 text-sm font-medium"><AppstoreOutlined aria-hidden="true" />{t("zdx.label.nav.tools")}</div>
        <div className="mt-auto pt-10 px-2 text-xs text-[#aab7d3]">{t("zdx.text.app.local")}</div>
      </aside>

      <main className="min-w-0 flex-1 px-5 py-6 md:px-10 md:py-9">
        <header className="flex items-start justify-between gap-4 border-b border-[#dfe5f0] dark:border-[#34415a] pb-6">
          <div><p className="m-0 text-sm text-[#667491] dark:text-[#aab7cf]">{t("zdx.text.app.welcome")}</p><h1 className="m-0 mt-1 text-3xl font-semibold tracking-tight">{t("zdx.label.nav.tools")}</h1></div>
          <div className="flex flex-wrap justify-end items-center gap-2">
          <label htmlFor="theme-mode" className="sr-only">{t("zdx.label.theme.title")}</label>
          <select id="theme-mode" value={mode} onChange={(event) => setMode(event.target.value as ThemeMode)} className="rounded-lg border border-[#d6deeb] bg-white px-3 py-2 text-sm text-[#405170] dark:bg-[#202b40] dark:border-[#46546b] dark:text-[#e7ecf7] cursor-pointer focus-visible:outline-2 focus-visible:outline-[#8193ff]">
            <option value="system">{t("zdx.label.theme.system")}</option><option value="light">{t("zdx.label.theme.light")}</option><option value="dark">{t("zdx.label.theme.dark")}</option>
          </select>
          <button type="button" onClick={() => setLanguage(language === "en" ? "zh-CN" : "en")} className="rounded-lg border border-[#d6deeb] bg-white px-3 py-2 text-sm text-[#405170] dark:bg-[#202b40] dark:border-[#46546b] dark:text-[#e7ecf7] cursor-pointer hover:bg-[#eef2fa] dark:hover:bg-[#2d3b55] focus-visible:outline-2 focus-visible:outline-[#536be2]" aria-label={t("zdx.label.language.switch")}>
            <GlobalOutlined aria-hidden="true" /> <span className="ml-1">{t("zdx.label.language.toggle")}</span>
          </button>
          </div>
        </header>

        <section className="mt-8 max-w-6xl">
          <div className="flex items-center gap-3"><div className="flex h-11 w-11 items-center justify-center rounded-xl bg-[#e6ebff] text-[#4c5fda] text-xl"><CodeOutlined aria-hidden="true" /></div><div><h2 className="m-0 text-xl font-semibold">{t("zdx.label.json.title")}</h2><p className="m-0 mt-1 text-sm text-[#687895] dark:text-[#aab7cf]">{t("zdx.text.json.description")}</p></div></div>
          <div className="mt-6 rounded-2xl border border-[#dfe5f0] bg-white dark:bg-[#1b2537] dark:border-[#34415a] overflow-hidden">
            <div className="grid md:grid-cols-2 md:divide-x divide-[#e7ebf3] dark:divide-[#34415a]">
              <div className="p-5"><label htmlFor="json-input" className="block text-sm font-semibold">{t("zdx.label.json.input")}</label><textarea id="json-input" value={input} onChange={(event) => { setInput(event.target.value); setOutput(""); setError(""); setCopied(false); }} placeholder={t("zdx.placeholder.json.input")} spellCheck={false} className="mt-3 block h-65 w-full box-border resize-y rounded-xl border border-[#dfe5f0] bg-[#f8faff] dark:bg-[#111827] dark:border-[#46546b] dark:text-[#e7ecf7] dark:placeholder:text-[#a0acc1] p-4 font-mono text-sm leading-6 text-[#293754] outline-none focus:border-[#6073e9]" /></div>
              <div className="p-5 border-t md:border-t-0 border-[#e7ebf3] dark:border-[#34415a]"><label htmlFor="json-output" className="block text-sm font-semibold">{t("zdx.label.json.output")}</label><textarea id="json-output" value={output} readOnly placeholder={t("zdx.placeholder.json.output")} spellCheck={false} className="mt-3 block h-65 w-full box-border resize-y rounded-xl border border-[#dfe5f0] bg-[#f8faff] dark:bg-[#111827] dark:border-[#46546b] dark:text-[#e7ecf7] dark:placeholder:text-[#a0acc1] p-4 font-mono text-sm leading-6 text-[#293754] outline-none focus:border-[#6073e9]" /></div>
            </div>
            <div className="flex flex-wrap items-center gap-2 border-t border-[#e7ebf3] dark:border-[#34415a] px-5 py-4"><Button type="primary" onClick={() => transform(false)}>{t("zdx.label.json.format")}</Button><Button onClick={() => transform(true)}>{t("zdx.label.json.compact")}</Button><Button disabled={!output} onClick={() => void copy()}>{t("zdx.label.json.copy")}</Button><span className="ml-auto text-sm text-[#527a67] dark:text-[#8ed9ad]" role="status">{copied ? t("zdx.text.json.copied") : ""}</span></div>
          </div>
          {error && <p role="alert" className="mt-3 text-sm text-[#bb3d4a] dark:text-[#ff8b98]">{t(error)}</p>}
          <p className="mt-5 text-xs leading-5 text-[#7786a0] dark:text-[#aab7cf]">{t("zdx.text.json.privacy")}</p>
        </section>
      </main>
    </div>
  );
}
