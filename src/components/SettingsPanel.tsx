import { useState, useEffect, useCallback } from "react";
import {
	getSettings,
	setShortcut,
	setAutostart,
	setClipboardLimit,
	setNotificationSettings,
	testNotification,
	setAutoPaste,
	setImageRetentionDays,
	cleanupImagesNow,
	setTelemetryEnabled,
	getDiagnostics,
	exportDiagnostics,
	openDiagnosticsFolder,
} from "../lib/tauri";
import type { NotificationSettings, Diagnostics } from "../lib/tauri";
import {
	Settings,
	Keyboard,
	Check,
	AlertCircle,
	Power,
	List,
	TriangleAlert,
	Bell,
	BellRing,
	Eye,
	EyeOff,
	ClipboardPaste,
	Image as ImageIcon,
	Activity,
	Download,
	FolderOpen,
	Brush,
} from "lucide-react";

function formatBytes(bytes: number): string {
	if (bytes < 1024) return `${bytes} B`;
	if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
	if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
	return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function formatAgo(seconds: number | null): string {
	if (seconds === null) return "never";
	if (seconds < 60) return `${seconds}s ago`;
	if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
	return `${Math.floor(seconds / 3600)}h ago`;
}

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

	// Notification settings
	const [notifEnabled, setNotifEnabled] = useState(true);
	const [notifShowContent, setNotifShowContent] = useState(true);

	// Auto-paste setting
	const [autoPaste, setAutoPasteState] = useState(false);

	// Image storage + diagnostics
	const [retentionDays, setRetentionDaysState] = useState(30);
	const [telemetryOn, setTelemetryOn] = useState(true);
	const [diag, setDiag] = useState<Diagnostics | null>(null);
	const [cleanupMsg, setCleanupMsg] = useState("");
	const [exportPath, setExportPath] = useState("");
	const [exportError, setExportError] = useState("");

	const refreshDiagnostics = useCallback(() => {
		getDiagnostics()
			.then(setDiag)
			.catch((err) => console.error("Failed to load diagnostics:", err));
	}, []);

	useEffect(() => {
		refreshDiagnostics();
		const t = setInterval(refreshDiagnostics, 5000);
		return () => clearInterval(t);
	}, [refreshDiagnostics]);

	useEffect(() => {
		getSettings().then((s) => {
			setCurrentShortcut(s.shortcut);
			setAutostartState(s.autostart);
			setClipboardLimitState(s.clipboardLimit ?? 20);
			if (s.notification) {
				setNotifEnabled(s.notification.enabled);
				setNotifShowContent(s.notification.showContent);
			}
			setAutoPasteState(s.autoPaste ?? false);
			setRetentionDaysState(s.imageRetentionDays ?? 30);
			setTelemetryOn(s.telemetryEnabled ?? true);
		});
	}, []);

	const saveNotificationSettings = useCallback(
		(overrides: Partial<NotificationSettings> = {}) => {
			const settings: NotificationSettings = {
				enabled: overrides.enabled ?? notifEnabled,
				showContent: overrides.showContent ?? notifShowContent,
			};
			setNotificationSettings(settings).catch((err) =>
				console.error("Failed to save notification settings:", err),
			);
		},
		[notifEnabled, notifShowContent],
	);

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

				{/* Auto-paste setting */}
				<div className="mb-4">
					<div className="mb-2 flex items-center gap-2">
						<ClipboardPaste size={13} className="text-text-secondary" />
						<span className="text-text-primary text-[12px] font-medium">Auto Paste</span>
					</div>

					<div className="bg-bg-secondary border-border rounded-(--radius-card) border p-3">
						<div className="flex items-center justify-between">
							<span className="text-text-secondary text-[11px]">Paste selected item automatically</span>
							<button
								onClick={() => {
									const next = !autoPaste;
									setAutoPasteState(next);
									setAutoPaste(next).catch((err) =>
										console.error("Failed to save auto-paste setting:", err),
									);
								}}
								className={`relative h-5 w-9 cursor-pointer rounded-full transition-colors ${autoPaste ? "bg-accent" : "bg-bg-active"}`}
							>
								<span className={`absolute top-0.5 left-0.5 h-4 w-4 rounded-full bg-white transition-transform ${autoPaste ? "translate-x-4" : ""}`} />
							</button>
						</div>
					</div>
					<p className="text-text-muted mt-2 text-[10px]">Simulates Ctrl+V in the previous window after selecting an item. Requires xdotool (X11). Note: terminals use Ctrl+Shift+V, so pasting there stays manual.</p>
				</div>

				{/* Copy Notification setting */}
				<div className="mb-4">
					<div className="mb-2 flex items-center gap-2">
						<Bell size={13} className="text-text-secondary" />
						<span className="text-text-primary text-[12px] font-medium">Copy Notification</span>
					</div>

					<div className="bg-bg-secondary border-border rounded-(--radius-card) border p-3">
						{/* Enable toggle */}
						<div className="flex items-center justify-between">
							<span className="text-text-secondary text-[11px]">Show notification on copy</span>
							<button
								onClick={() => {
									const next = !notifEnabled;
									setNotifEnabled(next);
									saveNotificationSettings({ enabled: next });
								}}
								className={`relative h-5 w-9 cursor-pointer rounded-full transition-colors ${notifEnabled ? "bg-accent" : "bg-bg-active"}`}
							>
								<span className={`absolute top-0.5 left-0.5 h-4 w-4 rounded-full bg-white transition-transform ${notifEnabled ? "translate-x-4" : ""}`} />
							</button>
						</div>

						{notifEnabled && (
							<>
								{/* Show content toggle */}
								<div className="mt-3 flex items-center justify-between border-t border-border pt-3">
									<div className="flex items-center gap-1.5">
										{notifShowContent ? <Eye size={11} className="text-text-secondary" /> : <EyeOff size={11} className="text-text-secondary" />}
										<span className="text-text-secondary text-[11px]">Show copied content in notification</span>
									</div>
									<button
										onClick={() => {
											const next = !notifShowContent;
											setNotifShowContent(next);
											saveNotificationSettings({ showContent: next });
										}}
										className={`relative h-5 w-9 cursor-pointer rounded-full transition-colors ${notifShowContent ? "bg-accent" : "bg-bg-active"}`}
									>
										<span className={`absolute top-0.5 left-0.5 h-4 w-4 rounded-full bg-white transition-transform ${notifShowContent ? "translate-x-4" : ""}`} />
									</button>
								</div>

								{/* Test notification button */}
								<div className="mt-3 border-t border-border pt-3">
									<button
										onClick={async () => {
											try {
												await testNotification();
											} catch (err) {
												console.error("Failed to send test notification:", err);
											}
										}}
										className="bg-bg-hover text-text-secondary hover:text-text-primary flex w-full cursor-pointer items-center justify-center gap-1.5 rounded py-1.5 text-[11px] transition-colors"
									>
										<BellRing size={11} />
										Send test notification
									</button>
								</div>
							</>
						)}
					</div>
					<p className="text-text-muted mt-2 text-[10px]">Uses your system's notification handler to show a brief alert when content is copied.</p>
				</div>
				{/* Image storage setting */}
				<div className="mb-4">
					<div className="mb-2 flex items-center gap-2">
						<ImageIcon size={13} className="text-text-secondary" />
						<span className="text-text-primary text-[12px] font-medium">Image Storage</span>
					</div>

					<div className="bg-bg-secondary border-border rounded-(--radius-card) border p-3">
						<div className="mb-2 flex items-center justify-between">
							<span className="text-text-secondary text-[11px]">Delete copied images after</span>
						</div>
						<div className="flex items-center gap-2">
							<input
								type="number"
								min={0}
								value={retentionDays}
								onChange={(e) => setRetentionDaysState(Math.max(0, parseInt(e.target.value) || 0))}
								onBlur={async (e) => {
									const val = Math.max(0, parseInt(e.target.value) || 0);
									setRetentionDaysState(val);
									try {
										await setImageRetentionDays(val);
									} catch (err) {
										console.error("Failed to save image retention:", err);
									}
								}}
								className="bg-bg-active text-text-primary border-border w-20 rounded border px-2 py-1 text-center font-mono text-[13px] font-semibold focus:outline-none"
							/>
							<span className="text-text-muted text-[10px]">days {retentionDays === 0 && "(never)"}</span>
						</div>

						{diag && (
							<div className="text-text-secondary mt-3 border-t border-border pt-3 text-[11px]">
								<div className="flex justify-between">
									<span>Images on disk</span>
									<span className="text-text-primary font-mono">
										{diag.storage.imageFiles} · {formatBytes(diag.storage.imageBytes)}
									</span>
								</div>
								{diag.storage.orphanFiles > 0 && (
									<div className="mt-1 flex justify-between text-yellow-400">
										<span>Not in history (leftovers)</span>
										<span className="font-mono">
											{diag.storage.orphanFiles} · {formatBytes(diag.storage.orphanBytes)}
										</span>
									</div>
								)}
							</div>
						)}

						<button
							onClick={async () => {
								setCleanupMsg("Cleaning…");
								try {
									const r = await cleanupImagesNow();
									setCleanupMsg(r.removedFiles > 0 ? `Removed ${r.removedFiles} image${r.removedFiles !== 1 ? "s" : ""}, freed ${formatBytes(r.freedBytes)}` : "Nothing to clean up");
									refreshDiagnostics();
								} catch (err) {
									setCleanupMsg(`Cleanup failed: ${String(err)}`);
								}
							}}
							className="bg-bg-hover text-text-secondary hover:text-text-primary mt-3 flex w-full cursor-pointer items-center justify-center gap-1.5 rounded py-1.5 text-[11px] transition-colors"
						>
							<Brush size={11} />
							Clean up images now
						</button>
						{cleanupMsg && <p className="text-text-muted mt-2 text-center text-[10px]">{cleanupMsg}</p>}
					</div>
					<p className="text-text-muted mt-2 text-[10px]">Unpinned images older than this are deleted from history and disk automatically. "Clear all" also deletes the image files. Pinned images are always kept. Set 0 to keep images until they fall out of history.</p>
				</div>

				{/* Diagnostics */}
				<div className="mb-4">
					<div className="mb-2 flex items-center gap-2">
						<Activity size={13} className="text-text-secondary" />
						<span className="text-text-primary text-[12px] font-medium">Diagnostics</span>
					</div>

					<div className="bg-bg-secondary border-border rounded-(--radius-card) border p-3">
						<div className="flex items-center justify-between gap-3">
							<span className="text-text-secondary text-[11px]">Keep a local diagnostics log</span>
							<button
								onClick={() => {
									const next = !telemetryOn;
									setTelemetryOn(next);
									setTelemetryEnabled(next)
										.then(refreshDiagnostics)
										.catch((err) => console.error("Failed to toggle diagnostics:", err));
								}}
								className={`relative h-5 w-9 cursor-pointer shrink-0 rounded-full transition-colors ${telemetryOn ? "bg-accent" : "bg-bg-active"}`}
							>
								<span className={`absolute top-0.5 left-0.5 h-4 w-4 rounded-full bg-white transition-transform ${telemetryOn ? "translate-x-4" : ""}`} />
							</button>
						</div>

						{diag && (
							<div className="text-text-secondary mt-3 space-y-1 border-t border-border pt-3 text-[11px]">
								<div className="flex justify-between">
									<span>Clipboard watcher</span>
									<span className={`font-mono ${diag.watcher.secondsSinceLastPoll !== null && diag.watcher.secondsSinceLastPoll < 10 ? "text-green-400" : "text-danger"}`}>
										{diag.watcher.secondsSinceLastPoll !== null && diag.watcher.secondsSinceLastPoll < 10 ? "running" : "stalled"}
									</span>
								</div>
								<div className="flex justify-between">
									<span>Last capture</span>
									<span className="text-text-primary font-mono">{formatAgo(diag.watcher.secondsSinceLastCapture)}</span>
								</div>
								<div className="flex justify-between">
									<span>Captured this session</span>
									<span className="text-text-primary font-mono">{diag.watcher.capturesThisSession}</span>
								</div>
								{(diag.watcher.saveFailuresThisSession > 0 || diag.watcher.restartsThisSession > 0 || diag.watcher.recoveredPanicsThisSession > 0) && (
									<div className="flex justify-between text-yellow-400">
										<span>Errors / restarts / recovered crashes</span>
										<span className="font-mono">
											{diag.watcher.saveFailuresThisSession} / {diag.watcher.restartsThisSession} / {diag.watcher.recoveredPanicsThisSession}
										</span>
									</div>
								)}
								<div className="flex justify-between">
									<span>Log size</span>
									<span className="text-text-primary font-mono">{formatBytes(diag.telemetryBytes)}</span>
								</div>
							</div>
						)}

						<div className="mt-3 flex gap-2 border-t border-border pt-3">
							<button
								onClick={async () => {
									setExportError("");
									try {
										setExportPath(await exportDiagnostics());
									} catch (err) {
										setExportError(String(err));
									}
								}}
								className="bg-accent/20 text-accent hover:bg-accent/30 flex flex-1 cursor-pointer items-center justify-center gap-1.5 rounded py-1.5 text-[11px] font-medium transition-colors"
							>
								<Download size={11} />
								Export report
							</button>
							<button
								onClick={() => openDiagnosticsFolder(exportPath || undefined).catch((err) => console.error(err))}
								className="bg-bg-hover text-text-secondary hover:text-text-primary flex flex-1 cursor-pointer items-center justify-center gap-1.5 rounded py-1.5 text-[11px] transition-colors"
							>
								<FolderOpen size={11} />
								Open folder
							</button>
						</div>
						{exportPath && <p className="mt-2 text-[10px] break-all text-green-400">Saved to {exportPath}</p>}
						{exportError && <p className="text-danger mt-2 text-[10px]">{exportError}</p>}
					</div>
					<p className="text-text-muted mt-2 text-[10px]">Records what the app does (copies, pastes, saves, errors, crashes) to help find bugs. Never includes the text or images you copy, and is never uploaded. To report a bug, export a report and attach it to a GitHub issue.</p>
				</div>
			</div>
		</div>
	);
}
