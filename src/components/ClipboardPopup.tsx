import { useState, useEffect, useCallback, useRef } from "react";
import type { ClipboardItem } from "../types/clipboard";
import { getHistory, searchHistory, clearHistory, hideWindow, pasteItem, startDrag, recordUiEvent } from "../lib/tauri";
import SearchBar from "./SearchBar";
import ClipboardItemCard from "./ClipboardItemCard";
import EmptyState from "./EmptyState";
import { Maximize2, Minimize2, X, Trash2, GripHorizontal, Settings } from "lucide-react";
import SettingsPanel from "./SettingsPanel";

interface Props {
	isFullscreen: boolean;
	onToggleFullscreen: () => void;
}

export default function ClipboardPopup({ isFullscreen, onToggleFullscreen }: Props) {
	const [items, setItems] = useState<ClipboardItem[]>([]);
	const [search, setSearch] = useState("");
	const [loading, setLoading] = useState(true);
	const [selectedIndex, setSelectedIndex] = useState(-1);
	const [showSettings, setShowSettings] = useState(false);
	const listRef = useRef<HTMLDivElement>(null);
	// Only log a fetch failure when it starts / changes, not on every 1s poll.
	const lastFetchError = useRef<string | null>(null);

	const fetchItems = useCallback(async () => {
		try {
			const data = search.trim() ? await searchHistory(search.trim()) : await getHistory(100, 0);
			setItems(data);
			if (lastFetchError.current !== null) {
				recordUiEvent("history_fetch_recovered");
				lastFetchError.current = null;
			}
		} catch (err) {
			console.error("Failed to fetch clipboard history:", err);
			const msg = String(err);
			if (lastFetchError.current !== msg) {
				lastFetchError.current = msg;
				recordUiEvent("history_fetch_failed", { error: msg });
			}
		} finally {
			setLoading(false);
		}
	}, [search]);

	useEffect(() => {
		fetchItems();
	}, [fetchItems]);

	// Poll for new items every 1 second
	useEffect(() => {
		const interval = setInterval(() => {
			fetchItems();
		}, 1000);
		return () => clearInterval(interval);
	}, [fetchItems]);

	// Keyboard navigation (cycles from bottom to top and vice versa)
	useEffect(() => {
		const handler = (e: KeyboardEvent) => {
			if (e.key === "ArrowDown") {
				e.preventDefault();
				setSelectedIndex((prev) => (prev + 1) % items.length);
			} else if (e.key === "ArrowUp") {
				e.preventDefault();
				setSelectedIndex((prev) => (prev - 1 + items.length) % items.length);
			} else if (e.key === "Enter" && selectedIndex >= 0 && selectedIndex < items.length) {
				e.preventDefault();
				pasteItem(items[selectedIndex].id).catch((err) => console.error("Paste failed:", err));
			}
		};
		document.addEventListener("keydown", handler);
		return () => document.removeEventListener("keydown", handler);
	}, [items, selectedIndex]);

	// Scroll selected item into view
	useEffect(() => {
		if (selectedIndex >= 0 && listRef.current) {
			const el = listRef.current.querySelector(`[data-index="${selectedIndex}"]`);
			el?.scrollIntoView({ block: "nearest" });
		}
	}, [selectedIndex]);

	// Reset selection when items change
	useEffect(() => {
		setSelectedIndex(-1);
	}, [search]);

	const handleClear = async () => {
		await clearHistory();
		fetchItems();
	};

	const pinnedItems = items.filter((i) => i.pinned);
	const unpinnedItems = items.filter((i) => !i.pinned);

	const handleDragStart = async (e: React.MouseEvent) => {
		if (e.button !== 0) return;
		await startDrag();
	};

	// Build flat ordered list matching render order (pinned first, then recent)
	const orderedItems = [...pinnedItems, ...unpinnedItems];

	if (showSettings) {
		return <SettingsPanel onClose={() => setShowSettings(false)} />;
	}

	return (
		<>
			{/* Header */}
			<div className={`bg-bg-primary flex shrink-0 items-center justify-between ${isFullscreen ? "px-6 py-2" : "px-4 py-1"}`}>
				<div className={`relative flex w-full flex-row justify-between ${isFullscreen ? "py-2" : "py-3"}`}>
					<span className={`text-text-primary font-semibold ${isFullscreen ? "text-[12px]" : "text-[13px]"}`}>Clipboard history</span>
					{!isFullscreen && (
						<div onMouseDown={handleDragStart} className="absolute top-0 right-1/2 flex shrink-0 translate-x-1/2 cursor-grab items-center justify-center gap-0 p-0 select-none active:cursor-grabbing">
							<GripHorizontal size={20} className="text-text-secondary p-0" />
							<GripHorizontal size={20} className="text-text-secondary -mx-0.5" />
						</div>
					)}
					<div className="flex items-center gap-1">
						<button onClick={() => setShowSettings(true)} className="text-text-muted hover:text-text-primary hover:bg-bg-hover cursor-pointer rounded p-1 transition-colors" title="Settings">
							<Settings size={12} />
						</button>
						<button onClick={handleClear} className="text-text-muted hover:text-danger hover:bg-bg-hover flex cursor-pointer items-center gap-1 rounded px-2 py-0.5 text-[11px] transition-colors" title="Clear all unpinned items">
							<Trash2 size={11} />
							Clear all
						</button>
						<button onClick={onToggleFullscreen} className="text-text-muted hover:text-text-primary hover:bg-bg-hover cursor-pointer rounded p-1 transition-colors" title={isFullscreen ? "Windowed mode" : "Fullscreen mode"}>
							{isFullscreen ? <Minimize2 size={12} /> : <Maximize2 size={12} />}
						</button>
						<button onClick={() => hideWindow()} className="text-text-muted hover:text-text-primary hover:bg-bg-hover cursor-pointer rounded p-1 transition-colors" title="Close">
							<X size={12} />
						</button>
					</div>
				</div>
			</div>

			{/* Search */}
			<div className={`shrink-0 ${isFullscreen ? "mx-auto w-full max-w-4xl pt-2 pb-3" : "px-3 pt-1 pb-2"}`}>
				<SearchBar value={search} onChange={setSearch} large={isFullscreen} />
			</div>

			{/* Items list */}
			<div ref={listRef} className={`flex-1 overflow-y-auto ${isFullscreen ? "px-6 pb-2" : "px-3 pb-2"}`}>
				{loading ? (
					<div className="flex h-32 items-center justify-center">
						<div className="border-accent h-5 w-5 animate-spin rounded-full border-2 border-t-transparent" />
					</div>
				) : items.length === 0 ? (
					<EmptyState hasSearch={search.length > 0} />
				) : (
					<div className={isFullscreen ? "mx-auto max-w-4xl space-y-2" : "space-y-1"}>
						{/* Pinned section */}
						{pinnedItems.length > 0 && !isFullscreen && (
							<>
								<div className="flex items-center gap-2 px-1 pt-1 pb-0.5">
									<span className="text-text-muted text-[10px] font-medium tracking-widest uppercase">Pinned</span>
									<div className="bg-border-subtle h-px flex-1" />
								</div>
								{pinnedItems.map((item) => {
									const idx = orderedItems.indexOf(item);
									return <ClipboardItemCard key={item.id} item={item} onUpdate={fetchItems} isSelected={idx === selectedIndex} dataIndex={idx} compact={isFullscreen} />;
								})}
							</>
						)}

						{/* Recent section */}
						{unpinnedItems.length > 0 && pinnedItems.length > 0 && !isFullscreen && (
							<div className="flex items-center gap-2 px-1 pt-2 pb-0.5">
								<span className="text-text-muted text-[10px] font-medium tracking-widest uppercase">Recent</span>
								<div className="bg-border-subtle h-px flex-1" />
							</div>
						)}
						{unpinnedItems.map((item) => {
							const idx = orderedItems.indexOf(item);
							return <ClipboardItemCard key={item.id} item={item} onUpdate={fetchItems} isSelected={idx === selectedIndex} dataIndex={idx} compact={isFullscreen} />;
						})}
					</div>
				)}
			</div>

			{/* Footer: item count + shortcut hint */}
			{items.length > 0 && (
				<div className="text-text-muted border-border-subtle flex shrink-0 items-center justify-between border-t px-4 py-1.5 text-[10px]">
					<span>
						{items.length} item{items.length !== 1 ? "s" : ""}
					</span>
					<span>Ctrl+Shift+V to toggle</span>
				</div>
			)}
		</>
	);
}
