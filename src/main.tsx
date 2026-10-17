import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./index.css";
import App from "./App.tsx";
import { recordUiEvent } from "./lib/tauri";

window.addEventListener("error", (e) => {
	recordUiEvent("error", { message: String(e.message), source: e.filename, line: e.lineno });
});
window.addEventListener("unhandledrejection", (e) => {
	recordUiEvent("unhandled_rejection", { reason: String(e.reason) });
});

createRoot(document.getElementById("root")!).render(
	<StrictMode>
		<App />
	</StrictMode>,
);
