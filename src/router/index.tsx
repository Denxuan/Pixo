import AppShell from "@/layout/AppShell";
import ClipboardPage from "@/pages/Clipboard";
import SystemSettingsPage from "@/pages/SystemSettings";

export default function AppRouter() {
  return (
    <HashRouter>
      <Routes>
        <Route element={<AppShell />}>
          <Route index element={<Navigate to="/clipboard" replace />} />
          <Route path="/clipboard" element={<ClipboardPage />} />
          <Route path="/clipboard/settings" element={<Navigate to="/settings/clipboard" replace />} />
          <Route path="/settings/:section?" element={<SystemSettingsPage />} />
          <Route path="*" element={<Navigate to="/clipboard" replace />} />
        </Route>
      </Routes>
    </HashRouter>
  );
}
