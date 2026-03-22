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

export interface AppSettings {
	shortcut: string;
	autostart: boolean;
	clipboardLimit: number;
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
