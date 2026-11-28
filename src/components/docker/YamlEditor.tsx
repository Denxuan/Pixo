import { yaml } from "@codemirror/lang-yaml";
import { EditorView } from "@codemirror/view";
import CodeMirror from "@uiw/react-codemirror";

import { useSettingsStore } from "@/stores/settings";
import { inkFrameStyle, isDarkTheme } from "@/utils/theme";

const extensions = [yaml(), EditorView.lineWrapping];

export default function YamlEditor({
  value,
  label,
  placeholder,
  onChange,
}: {
  value: string;
  label: string;
  placeholder: string;
  onChange: (value: string) => void;
}) {
  const { token } = antdTheme.useToken();
  const mode = useSettingsStore((state) => state.mode);
  const systemDark = useSyncExternalStore(
    (onStoreChange) => {
      const query = window.matchMedia("(prefers-color-scheme: dark)");
      query.addEventListener("change", onStoreChange);
      return () => query.removeEventListener("change", onStoreChange);
    },
    () => window.matchMedia("(prefers-color-scheme: dark)").matches,
    () => false,
  );

  return (
    <CodeMirror
      value={value}
      height="min(320px, calc(100vh - 420px))"
      minHeight="180px"
      theme={isDarkTheme(mode, systemDark) ? "dark" : "light"}
      extensions={extensions}
      indentWithTab
      placeholder={placeholder}
      aria-label={label}
      onChange={onChange}
      style={{ overflow: "hidden", ...inkFrameStyle(token, token.borderRadius) }}
    />
  );
}
