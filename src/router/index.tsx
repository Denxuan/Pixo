import AppShell from "@/layout/AppShell";
import ClipboardPage from "@/pages/Clipboard";
import ClipboardSettingsPage from "@/pages/ClipboardSettings";
import SystemSettingsPage from "@/pages/SystemSettings";

export default function AppRouter() {
  return (
    <HashRouter>
      <Routes>
        <Route element={<AppShell />}>
          <Route index element={<Navigate to="/clipboard" replace />} />
          <Route path="/clipboard" element={<ClipboardPage />} />
          <Route path="/clipboard/settings" element={<ClipboardSettingsPage />} />
          <Route path="/settings" element={<SystemSettingsPage />} />
          <Route path="*" element={<Navigate to="/clipboard" replace />} />
        </Route>
      </Routes>
    </HashRouter>
  );
}
