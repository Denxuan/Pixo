import App from "@/App";
import "@/lang";
import "@/scrollbar.css";
import "virtual:uno.css";

createRoot(document.getElementById("root") as HTMLElement).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
