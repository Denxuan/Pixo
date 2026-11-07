import AppShell from "@/layout/AppShell";
import ClipboardPage from "@/pages/Clipboard";
import JsonTool from "@/pages/JsonTool";

export default function AppRouter() {
  return (
    <HashRouter>
      <Routes>
        <Route element={<AppShell />}>
          <Route index element={<Navigate to="/json" replace />} />
          <Route path="/json" element={<JsonTool />} />
          <Route path="/clipboard" element={<ClipboardPage />} />
          <Route path="*" element={<Navigate to="/json" replace />} />
        </Route>
      </Routes>
    </HashRouter>
  );
}
