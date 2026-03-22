import { useState, useEffect, useCallback } from "react";
import { getSettings, setShortcut, setAutostart, setClipboardLimit } from "../lib/tauri";
import { Settings, Keyboard, Check, AlertCircle, Power, List, TriangleAlert } from "lucide-react";

interface Props {
	onClose: () => void;
}

// Map browser key events to Tauri shortcut format
function keyEventToShortcutString(e: KeyboardEvent): string | null {
	const parts: string[] = [];

	if (e.ctrlKey) parts.push("Ctrl");
	if (e.altKey) parts.push("Alt");
	if (e.shiftKey) parts.push("Shift");
	if (e.metaKey) parts.push("Super");

	// Ignore if only modifiers are pressed
	const key = e.key;
	if (["Control", "Alt", "Shift", "Meta"].includes(key)) return null;

	// Map common keys
	const keyMap: Record<string, string> = {
		" ": "Space",
		ArrowUp: "Up",
		ArrowDown: "Down",
		ArrowLeft: "Left",
		ArrowRight: "Right",
		Delete: "Delete",
		Backspace: "Backspace",
		Enter: "Enter",
		Tab: "Tab",
		Escape: "Escape",
	};

	const mappedKey = keyMap[key] || (key.length === 1 ? key.toUpperCase() : key);
	parts.push(mappedKey);

	// Need at least one modifier
	if (parts.length < 2) return null;

	return parts.join("+");
}

export default function SettingsPanel({ onClose }: Props) {
	const [currentShortcut, setCurrentShortcut] = useState("");
	const [autostart, setAutostartState] = useState(false);
	const [clipboardLimit, setClipboardLimitState] = useState(20);
	const [recording, setRecording] = useState(false);
	const [recordedKeys, setRecordedKeys] = useState("");
	const [status, setStatus] = useState<"idle" | "success" | "error">("idle");
	const [errorMsg, setErrorMsg] = useState("");

	useEffect(() => {
		getSettings().then((s) => {
			setCurrentShortcut(s.shortcut);
			setAutostartState(s.autostart);
			setClipboardLimitState(s.clipboardLimit ?? 20);
		});
	}, []);

	const handleKeyDown = useCallback(
		(e: KeyboardEvent) => {
			if (!recording) return;
			e.preventDefault();
			e.stopPropagation();

			const shortcut = keyEventToShortcutString(e);
			if (shortcut) {
				setRecordedKeys(shortcut);
			}
		},
		[recording],
	);

	useEffect(() => {
		if (recording) {
			document.addEventListener("keydown", handleKeyDown, true);
			return () => document.removeEventListener("keydown", handleKeyDown, true);
		}
	}, [recording, handleKeyDown]);

	const handleSave = async () => {
		if (!recordedKeys) return;
		try {
			await setShortcut(recordedKeys);
			setCurrentShortcut(recordedKeys);
			setRecording(false);
			setRecordedKeys("");
			setStatus("success");
			setErrorMsg("");
			setTimeout(() => setStatus("idle"), 2000);
		} catch (err) {
			setStatus("error");
			setErrorMsg(String(err));
			setTimeout(() => setStatus("idle"), 3000);
		}
	};

	return (
		<div className="flex h-full flex-col">
			{/* Header */}
			<div className="bg-bg-primary flex shrink-0 items-center justify-between px-4 py-3">
				<div className="flex items-center gap-2">
					<Settings size={14} className="text-text-secondary" />
					<span className="text-text-primary text-[13px] font-semibold">Settings</span>
				</div>
				<button onClick={onClose} className="text-text-muted hover:text-text-primary cursor-pointer text-[11px]">
					Back
				</button>
			</div>

			{/* Content */}
			<div className="flex-1 overflow-y-auto px-4 py-3">
				{/* Shortcut setting */}
				<div className="mb-4">
					<div className="mb-2 flex items-center gap-2">
						<Keyboard size={13} className="text-text-secondary" />
						<span className="text-text-primary text-[12px] font-medium">Toggle Shortcut</span>
					</div>

					<div className="bg-bg-secondary border-border rounded-(--radius-card) border p-3">
						<div className="mb-2 flex items-center justify-between">
							<span className="text-text-secondary text-[11px]">Current shortcut:</span>
							<kbd className="bg-bg-active text-text-primary rounded px-2 py-0.5 font-mono text-[11px]">{currentShortcut}</kbd>
						</div>

						{recording ? (
							<div className="mt-3">
								<div className="bg-bg-primary border-accent/40 rounded border p-3 text-center">
									<p className="text-text-secondary mb-2 text-[11px]">Press your desired key combination</p>
									<kbd className="bg-bg-active text-accent inline-block min-h-7 rounded px-3 py-1 font-mono text-[13px]">{recordedKeys || "..."}</kbd>
								</div>
								<div className="mt-2 flex gap-2">
									<button onClick={handleSave} disabled={!recordedKeys} className="bg-accent/20 text-accent hover:bg-accent/30 flex flex-1 cursor-pointer items-center justify-center gap-1 rounded py-1.5 text-[11px] font-medium transition-colors disabled:opacity-40">
										<Check size={12} />
										Save
									</button>
									<button
										onClick={() => {
											setRecording(false);
											setRecordedKeys("");
										}}
										className="bg-bg-hover text-text-secondary hover:text-text-primary flex-1 cursor-pointer rounded py-1.5 text-[11px] transition-colors"
									>
										Cancel
									</button>
								</div>
							</div>
						) : (
							<button onClick={() => setRecording(true)} className="bg-bg-hover text-text-secondary hover:text-text-primary mt-1 w-full cursor-pointer rounded py-1.5 text-[11px] transition-colors">
								Change shortcut
							</button>
						)}

						{status === "success" && (
							<div className="mt-2 flex items-center gap-1 text-[11px] text-green-400">
								<Check size={12} />
								Shortcut updated!
							</div>
						)}
						{status === "error" && (
							<div className="text-danger mt-2 flex items-center gap-1 text-[11px]">
								<AlertCircle size={12} />
								{errorMsg}
							</div>
						)}
					</div>

					<p className="text-text-muted mt-2 text-[10px]">Use a combination like Ctrl+Alt+V, Super+V, or Alt+Shift+C. Avoid shortcuts already used by your system or terminal.</p>
				</div>

				{/* Autostart setting */}
				<div className="mb-4">
					<div className="mb-2 flex items-center gap-2">
						<Power size={13} className="text-text-secondary" />
						<span className="text-text-primary text-[12px] font-medium">Start on Login</span>
					</div>

					<div className="bg-bg-secondary border-border rounded-(--radius-card) border p-3">
						<div className="flex items-center justify-between">
							<span className="text-text-secondary text-[11px]">Launch Recopied automatically when you log in</span>
							<button
								onClick={async () => {
									const next = !autostart;
									try {
										await setAutostart(next);
										setAutostartState(next);
									} catch (err) {
										console.error("Failed to toggle autostart:", err);
									}
								}}
								className={`relative h-5 w-9 cursor-pointer rounded-full transition-colors ${autostart ? "bg-accent" : "bg-bg-active"}`}
							>
								<span className={`absolute top-0.5 left-0.5 h-4 w-4 rounded-full bg-white transition-transform ${autostart ? "translate-x-4" : ""}`} />
							</button>
						</div>
					</div>
				</div>

				{/* Clipboard limit setting */}
				<div className="mb-4">
					<div className="mb-2 flex items-center gap-2">
						<List size={13} className="text-text-secondary" />
						<span className="text-text-primary text-[12px] font-medium">Clipboard Limit</span>
					</div>

					<div className="bg-bg-secondary border-border rounded-(--radius-card) border p-3">
						<div className="mb-2 flex items-center justify-between">
							<span className="text-text-secondary text-[11px]">Max items to keep in history</span>
						</div>
						<div className="flex items-center gap-2">
							<input
								type="number"
								min={1}
								value={clipboardLimit}
								onChange={(e) => {
									const val = Math.max(1, parseInt(e.target.value) || 1);
									setClipboardLimitState(val);
								}}
								onBlur={async (e) => {
									const val = Math.max(1, parseInt(e.target.value) || 1);
									setClipboardLimitState(val);
									try {
										await setClipboardLimit(val);
									} catch (err) {
										console.error("Failed to save clipboard limit:", err);
									}
								}}
								className="bg-bg-active text-text-primary border-border w-20 rounded border px-2 py-1 text-center font-mono text-[13px] font-semibold focus:outline-none"
							/>
							<span className="text-text-muted text-[10px]">items</span>
						</div>
						{clipboardLimit > 20 && (
							<div className="mt-2 flex items-start gap-1.5 rounded bg-yellow-500/10 px-2 py-1.5">
								<TriangleAlert size={11} className="mt-0.5 shrink-0 text-yellow-400" />
								<span className="text-[10px] text-yellow-400">Increase this at your own risk — large histories can slow down the app.</span>
							</div>
						)}
					</div>
					<p className="text-text-muted mt-2 text-[10px]">Older items are pruned automatically when the limit is reached. Pinned items are never deleted.</p>
				</div>
			</div>
		</div>
	);
}
