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

export async function setNotificationSettings(notification: NotificationSettings): Promise<void> {
	return invoke("set_notification_settings", { notification });
}

export async function testNotification(): Promise<void> {
	return invoke("test_notification");
}
