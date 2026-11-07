import { formatJson } from "@/utils/json";

export default function JsonTool() {
  const { t } = useTranslation();
  const [input, setInput] = useState("");
  const [output, setOutput] = useState("");
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);

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
    <section className="mt-5 flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center gap-3">
        <div className="flex h-11 w-11 items-center justify-center rounded-xl bg-[#e6ebff] text-xl text-[#4c5fda]"><CodeOutlined aria-hidden="true" /></div>
        <div>
          <h1 className="m-0 text-3xl font-semibold tracking-tight">{t("zdx.label.json.title")}</h1>
          <p className="m-0 mt-1 text-sm text-[#687895] dark:text-[#aab7cf]">{t("zdx.text.json.description")}</p>
        </div>
      </div>
      <div className="mt-5 flex min-h-0 flex-1 flex-col overflow-hidden rounded-2xl border border-[#dfe5f0] bg-white dark:border-[#34415a] dark:bg-[#1b2537]">
        <div className="grid min-h-0 flex-1 grid-rows-2 divide-[#e7ebf3] md:grid-cols-2 md:grid-rows-1 md:divide-x dark:divide-[#34415a]">
          <div className="flex min-h-0 min-w-0 flex-col p-5">
            <label htmlFor="json-input" className="block shrink-0 text-sm font-semibold">{t("zdx.label.json.input")}</label>
            <textarea id="json-input" value={input} onChange={(event) => { setInput(event.target.value); setOutput(""); setError(""); setCopied(false); }} placeholder={t("zdx.placeholder.json.input")} spellCheck={false} className="mt-3 block min-h-0 w-full flex-1 resize-none rounded-xl border border-[#dfe5f0] bg-[#f8faff] p-4 font-mono text-sm leading-6 text-[#293754] outline-none focus:border-[#6073e9] dark:border-[#46546b] dark:bg-[#111827] dark:text-[#e7ecf7] dark:placeholder:text-[#a0acc1]" />
          </div>
          <div className="flex min-h-0 min-w-0 flex-col border-t border-[#e7ebf3] p-5 md:border-t-0 dark:border-[#34415a]">
            <label htmlFor="json-output" className="block shrink-0 text-sm font-semibold">{t("zdx.label.json.output")}</label>
            <textarea id="json-output" value={output} readOnly placeholder={t("zdx.placeholder.json.output")} spellCheck={false} className="mt-3 block min-h-0 w-full flex-1 resize-none rounded-xl border border-[#dfe5f0] bg-[#f8faff] p-4 font-mono text-sm leading-6 text-[#293754] outline-none focus:border-[#6073e9] dark:border-[#46546b] dark:bg-[#111827] dark:text-[#e7ecf7] dark:placeholder:text-[#a0acc1]" />
          </div>
        </div>
        <div className="flex shrink-0 flex-wrap items-center gap-2 border-t border-[#e7ebf3] px-5 py-4 dark:border-[#34415a]">
          <Button type="primary" onClick={() => transform(false)}>{t("zdx.label.json.format")}</Button>
          <Button onClick={() => transform(true)}>{t("zdx.label.json.compact")}</Button>
          <Button disabled={!output} onClick={() => void copy()}>{t("zdx.label.json.copy")}</Button>
          <span className="ml-auto text-sm text-[#527a67] dark:text-[#8ed9ad]" role="status">{copied ? t("zdx.text.json.copied") : ""}</span>
        </div>
      </div>
      {error && <p role="alert" className="mt-3 shrink-0 text-sm text-[#bb3d4a] dark:text-[#ff8b98]">{t(error)}</p>}
      <p className="mt-3 shrink-0 text-xs leading-5 text-[#7786a0] dark:text-[#aab7cf]">{t("zdx.text.json.privacy")}</p>
    </section>
  );
}
