import { invoke } from "@tauri-apps/api/core";
import type { ClipboardItem } from "../types/clipboard";

export async function getHistory(limit: number = 50, offset: number = 0): Promise<ClipboardItem[]> {
	return invoke("get_history", { limit, offset });
}

export async function searchHistory(query: string): Promise<ClipboardItem[]> {
	return invoke("search_history", { query });
}

export async function pinItem(id: number): Promise<boolean> {
	return invoke("pin_item", { id });
}

export async function deleteItem(id: number): Promise<boolean> {
	return invoke("delete_item", { id });
}

export async function clearHistory(): Promise<boolean> {
	return invoke("clear_history");
}

export async function pasteItem(id: number): Promise<void> {
	return invoke("paste_item", { id });
}

export async function hideWindow(): Promise<void> {
	return invoke("hide_window");
}

export async function startDrag(): Promise<void> {
	return invoke("start_drag");
}

export async function setWindowMode(fullscreen: boolean): Promise<void> {
	return invoke("set_window_mode", { fullscreen });
}

export interface NotificationSettings {
	enabled: boolean;
	showContent: boolean;
}

export interface AppSettings {
	shortcut: string;
	autostart: boolean;
	clipboardLimit: number;
	notification: NotificationSettings;
	autoPaste: boolean;
	telemetryEnabled: boolean;
	imageRetentionDays: number;
}

export async function getSettings(): Promise<AppSettings> {
	return invoke("get_settings");
}

export async function setShortcut(shortcut: string): Promise<void> {
	return invoke("set_shortcut", { shortcut });
}

export async function setAutostart(enabled: boolean): Promise<void> {
	return invoke("set_autostart", { enabled });
}

export async function setClipboardLimit(limit: number): Promise<void> {
	return invoke("set_clipboard_limit", { limit });
}

export async function setAutoPaste(enabled: boolean): Promise<void> {
	return invoke("set_auto_paste", { enabled });
}

export async function setNotificationSettings(notification: NotificationSettings): Promise<void> {
	return invoke("set_notification_settings", { notification });
}

export async function testNotification(): Promise<void> {
	return invoke("test_notification");
}

export interface CleanupReport {
	removedItems: number;
	removedFiles: number;
	orphanFiles: number;
	freedBytes: number;
	errors: string[];
}

export interface StorageStats {
	textItems: number;
	imageItems: number;
	pinnedItems: number;
	oldestItem: string | null;
	newestItem: string | null;
	imageFiles: number;
	imageBytes: number;
	orphanFiles: number;
	orphanBytes: number;
	missingImageFiles: number;
	dbBytes: number;
}

export interface Diagnostics {
	telemetryEnabled: boolean;
	telemetryDir: string;
	telemetryBytes: number;
	session: string;
	watcher: {
		generation: number;
		secondsSinceLastPoll: number | null;
		secondsSinceLastCapture: number | null;
		capturesThisSession: number;
		saveFailuresThisSession: number;
		restartsThisSession: number;
		recoveredPanicsThisSession: number;
	};
	storage: StorageStats;
}

export async function setImageRetentionDays(days: number): Promise<void> {
	return invoke("set_image_retention_days", { days });
}

export async function cleanupImagesNow(): Promise<CleanupReport> {
	return invoke("cleanup_images_now");
}

export async function setTelemetryEnabled(enabled: boolean): Promise<void> {
	return invoke("set_telemetry_enabled", { enabled });
}

export async function getDiagnostics(): Promise<Diagnostics> {
	return invoke("get_diagnostics");
}

export async function exportDiagnostics(): Promise<string> {
	return invoke("export_diagnostics");
}

export async function openDiagnosticsFolder(path?: string): Promise<void> {
	return invoke("open_diagnostics_folder", { path: path ?? null });
}

/** Fire-and-forget: log a UI-side event into the local diagnostics log. */
export function recordUiEvent(event: string, data?: Record<string, unknown>): void {
	invoke("record_ui_event", { event, data: data ?? null }).catch(() => {});
}
