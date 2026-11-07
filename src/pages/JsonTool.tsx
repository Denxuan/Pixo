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
    <Flex vertical gap={16} className="min-h-0 flex-1 overflow-hidden">
      <div>
        <Typography.Title level={3} style={{ margin: 0 }}>{t("zdx.label.json.title")}</Typography.Title>
        <Typography.Paragraph type="secondary" style={{ marginBottom: 0 }}>{t("zdx.text.json.description")}</Typography.Paragraph>
      </div>
      <Flex gap={16} className="min-h-0 flex-1">
        <Flex vertical gap={8} className="min-h-0 min-w-0 flex-1">
          <Typography.Text strong>{t("zdx.label.json.input")}</Typography.Text>
          <Input.TextArea id="json-input" value={input} onChange={(event) => { setInput(event.target.value); setOutput(""); setError(""); setCopied(false); }} placeholder={t("zdx.placeholder.json.input")} spellCheck={false} style={{ resize: "none", height: "100%" }} className="min-h-0 flex-1" />
        </Flex>
        <Flex vertical gap={8} className="min-h-0 min-w-0 flex-1">
          <Typography.Text strong>{t("zdx.label.json.output")}</Typography.Text>
          <Input.TextArea id="json-output" value={output} readOnly placeholder={t("zdx.placeholder.json.output")} spellCheck={false} style={{ resize: "none", height: "100%" }} className="min-h-0 flex-1" />
        </Flex>
      </Flex>
      <Space wrap>
        <Button type="primary" onClick={() => transform(false)}>{t("zdx.label.json.format")}</Button>
        <Button onClick={() => transform(true)}>{t("zdx.label.json.compact")}</Button>
        <Button disabled={!output} onClick={() => void copy()}>{t("zdx.label.json.copy")}</Button>
        {copied && <Typography.Text type="success">{t("zdx.text.json.copied")}</Typography.Text>}
      </Space>
      {error && <Alert type="error" showIcon message={t(error)} />}
      <Typography.Text type="secondary">{t("zdx.text.json.privacy")}</Typography.Text>
    </Flex>
  );
}
