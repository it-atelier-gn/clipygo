<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { browser } from '$app/environment';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { getCurrentWebviewWindow, WebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { writeText, writeImageBase64 } from 'tauri-plugin-clipboard-api';

  type KindTag = 'text' | 'image';

  interface HistoryEntryView {
    id: string;
    timestamp: number;
    kind_tag: KindTag;
    preview: string;
    mime?: string;
    width?: number;
    height?: number;
    size_bytes: number;
    matched_pattern?: string;
    pinned: boolean;
    last_sent_to?: string;
  }

  interface Stats {
    items: number;
    bytes_used: number;
    bytes_cap: number;
    persisted_to_disk: boolean;
  }

  const PAGE_SIZE = 100;

  let entries: HistoryEntryView[] = [];
  let stats: Stats | null = null;
  let filterKind: 'all' | 'text' | 'image' = 'all';
  let pinnedOnly = false;
  let query = '';
  let thumbnails: Record<string, string> = {};
  let unlistenChanged: UnlistenFn | null = null;
  let unlistenFocus: UnlistenFn | null = null;
  let resendingId: string | null = null;
  let errorMsg = '';
  let selectedIndex = 0;
  let searchInput: HTMLInputElement | null = null;
  let rowEls: Record<string, HTMLDivElement> = {};

  async function refresh() {
    try {
      const filter = { kind: filterKind, query, pinned_only: pinnedOnly };
      entries = await invoke<HistoryEntryView[]>('history_list', { filter, offset: 0, limit: PAGE_SIZE });
      stats = await invoke<Stats>('history_stats');
      for (const e of entries) {
        if (e.kind_tag === 'image' && !thumbnails[e.id]) {
          loadThumbnail(e.id);
        }
      }
    } catch (e) {
      errorMsg = `Failed to load: ${e}`;
    }
  }

  async function loadThumbnail(id: string) {
    try {
      const b64 = await invoke<string>('history_get_image', { id });
      thumbnails = { ...thumbnails, [id]: `data:image/png;base64,${b64}` };
    } catch {
      /* ignore */
    }
  }

  async function togglePin(e: HistoryEntryView) {
    try {
      await invoke('history_pin', { id: e.id, pinned: !e.pinned });
      e.pinned = !e.pinned;
      entries = [...entries];
    } catch (err) {
      errorMsg = `Pin failed: ${err}`;
    }
  }

  async function deleteEntry(e: HistoryEntryView) {
    try {
      await invoke('history_delete', { id: e.id });
      entries = entries.filter((x) => x.id !== e.id);
      delete thumbnails[e.id];
      thumbnails = thumbnails;
      stats = await invoke<Stats>('history_stats');
    } catch (err) {
      errorMsg = `Delete failed: ${err}`;
    }
  }

  async function writePayloadToClipboard(payload: { kind: string; text?: string; image_base64?: string }) {
    if (payload.kind === 'text' && payload.text != null) {
      await invoke('history_suppress_next_text', { text: payload.text });
      await writeText(payload.text);
    } else if (payload.kind === 'image' && payload.image_base64) {
      await invoke('history_suppress_next_image_b64', { imageBase64: payload.image_base64 });
      await writeImageBase64(payload.image_base64);
    }
  }

  async function copyToClipboard(e: HistoryEntryView) {
    try {
      const payload = await invoke<{ kind: string; text?: string; image_base64?: string }>(
        'history_resend',
        { id: e.id },
      );
      await writePayloadToClipboard(payload);
    } catch (err) {
      errorMsg = `Copy failed: ${err}`;
    }
  }

  async function resend(e: HistoryEntryView) {
    resendingId = e.id;
    try {
      const payload = await invoke<{ kind: string; text?: string; image_base64?: string }>(
        'history_resend',
        { id: e.id },
      );
      await writePayloadToClipboard(payload);
      const main = await WebviewWindow.getByLabel('main');
      if (main) {
        await main.show();
        await main.setFocus();
      }
    } catch (err) {
      errorMsg = `Resend failed: ${err}`;
    } finally {
      resendingId = null;
    }
  }

  async function clearAll(includePinned: boolean) {
    if (!confirm(includePinned ? 'Clear ALL entries including pinned?' : 'Clear all unpinned entries?')) return;
    try {
      await invoke('history_clear', { includePinned });
      entries = [];
      thumbnails = {};
      stats = await invoke<Stats>('history_stats');
    } catch (err) {
      errorMsg = `Clear failed: ${err}`;
    }
  }

  function formatBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / 1024 / 1024).toFixed(1)} MB`;
  }

  function formatTime(ms: number): string {
    const d = new Date(ms);
    const today = new Date();
    const sameDay = d.toDateString() === today.toDateString();
    if (sameDay) {
      return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    }
    return d.toLocaleString([], {
      month: 'short',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    });
  }

  let searchDebounce: ReturnType<typeof setTimeout>;
  function onQueryInput(v: string) {
    query = v;
    selectedIndex = 0;
    clearTimeout(searchDebounce);
    searchDebounce = setTimeout(refresh, 200);
  }

  function clampSelection() {
    if (selectedIndex >= entries.length) selectedIndex = Math.max(0, entries.length - 1);
    if (selectedIndex < 0) selectedIndex = 0;
  }

  function scrollSelectedIntoView() {
    const sel = entries[selectedIndex];
    if (!sel) return;
    const el = rowEls[sel.id];
    if (el) el.scrollIntoView({ block: 'nearest' });
  }

  async function onKeyDown(ev: KeyboardEvent) {
    if (ev.key === 'Escape') {
      ev.preventDefault();
      if (query) {
        query = '';
        await refresh();
        searchInput?.focus();
      } else {
        await getCurrentWebviewWindow().hide();
      }
      return;
    }

    const onSearchField = ev.target === searchInput;
    const printable = ev.key.length === 1 && !ev.ctrlKey && !ev.metaKey && !ev.altKey;
    if (printable && !onSearchField) {
      searchInput?.focus();
      return;
    }

    switch (ev.key) {
      case 'ArrowDown':
        ev.preventDefault();
        if (entries.length) { selectedIndex = Math.min(entries.length - 1, selectedIndex + 1); scrollSelectedIntoView(); }
        break;
      case 'ArrowUp':
        ev.preventDefault();
        if (entries.length) { selectedIndex = Math.max(0, selectedIndex - 1); scrollSelectedIntoView(); }
        break;
      case 'Home':
        ev.preventDefault();
        selectedIndex = 0; scrollSelectedIntoView();
        break;
      case 'End':
        ev.preventDefault();
        selectedIndex = Math.max(0, entries.length - 1); scrollSelectedIntoView();
        break;
      case 'PageDown':
        ev.preventDefault();
        selectedIndex = Math.min(entries.length - 1, selectedIndex + 10); scrollSelectedIntoView();
        break;
      case 'PageUp':
        ev.preventDefault();
        selectedIndex = Math.max(0, selectedIndex - 10); scrollSelectedIntoView();
        break;
      case 'Enter': {
        const e = entries[selectedIndex];
        if (!e) return;
        ev.preventDefault();
        if (ev.ctrlKey || ev.metaKey) {
          await resend(e);
        } else {
          await copyToClipboard(e);
          await getCurrentWebviewWindow().hide();
        }
        break;
      }
      case 'Delete': {
        const e = entries[selectedIndex];
        if (!e) return;
        ev.preventDefault();
        await deleteEntry(e);
        clampSelection();
        break;
      }
      case 'p':
      case 'P':
        if (ev.ctrlKey || ev.metaKey) {
          const e = entries[selectedIndex];
          if (!e) return;
          ev.preventDefault();
          await togglePin(e);
        }
        break;
    }
  }

  onMount(async () => {
    if (!browser) return;
    await refresh();
    searchInput?.focus();
    unlistenChanged = await listen('history-changed', () => {
      refresh().then(clampSelection);
    });
    const win = getCurrentWebviewWindow();
    unlistenFocus = await win.onFocusChanged(({ payload: focused }) => {
      if (focused) searchInput?.focus();
    });
  });

  onDestroy(() => {
    if (unlistenChanged) unlistenChanged();
    if (unlistenFocus) unlistenFocus();
  });
</script>

<svelte:window on:keydown={onKeyDown} />

<div class="history-container">
  <div class="drag-strip" data-tauri-drag-region>
    <span class="drag-label" data-tauri-drag-region>history</span>
    <div class="drag-strip-actions" data-tauri-drag-region>
      {#if stats}
        <span class="stats-chip" data-tauri-drag-region>
          {stats.items} items · {formatBytes(stats.bytes_used)} / {formatBytes(stats.bytes_cap)}
          {stats.persisted_to_disk ? '· disk' : '· memory'}
        </span>
      {/if}
      <button class="btn-strip" data-tauri-drag-region="false" on:click={() => clearAll(false)}>Clear</button>
      <button class="btn-strip btn-strip-close" data-tauri-drag-region="false" on:click={() => getCurrentWebviewWindow().hide()}>✕</button>
    </div>
  </div>

  <div class="filter-bar">
    <input
      class="input search-input"
      placeholder="Search…"
      value={query}
      bind:this={searchInput}
      on:input={(e) => onQueryInput(e.currentTarget.value)}
    />
    <div class="filter-buttons">
      <button class="filter-btn" class:filter-active={filterKind === 'all'} on:click={() => { filterKind = 'all'; selectedIndex = 0; refresh(); }}>All</button>
      <button class="filter-btn" class:filter-active={filterKind === 'text'} on:click={() => { filterKind = 'text'; selectedIndex = 0; refresh(); }}>Text</button>
      <button class="filter-btn" class:filter-active={filterKind === 'image'} on:click={() => { filterKind = 'image'; selectedIndex = 0; refresh(); }}>Images</button>
      <button class="filter-btn" class:filter-active={pinnedOnly} on:click={() => { pinnedOnly = !pinnedOnly; selectedIndex = 0; refresh(); }}>📌 Pinned</button>
    </div>
  </div>

  {#if errorMsg}
    <div class="error-banner">{errorMsg} <button class="btn-strip" on:click={() => (errorMsg = '')}>✕</button></div>
  {/if}

  <div class="history-list">
    {#if entries.length === 0}
      <div class="empty-state">No history entries</div>
    {:else}
      {#each entries as e, i (e.id)}
        <div
          class="row"
          class:row-pinned={e.pinned}
          class:row-selected={i === selectedIndex}
          bind:this={rowEls[e.id]}
          on:mousedown={() => (selectedIndex = i)}
        >
          <div class="row-thumb">
            {#if e.kind_tag === 'image' && thumbnails[e.id]}
              <img src={thumbnails[e.id]} alt="" />
            {:else if e.kind_tag === 'image'}
              <div class="thumb-placeholder">🖼</div>
            {:else}
              <div class="thumb-placeholder">📋</div>
            {/if}
          </div>
          <div class="row-body">
            <div class="row-meta">
              <span class="row-time">{formatTime(e.timestamp)}</span>
              <span class="row-size">{formatBytes(e.size_bytes)}</span>
              {#if e.matched_pattern}
                <span class="chip chip-pattern" title={e.matched_pattern}>matched</span>
              {/if}
              {#if e.last_sent_to}
                <span class="chip chip-target">→ {e.last_sent_to}</span>
              {/if}
              {#if e.kind_tag === 'image' && e.width && e.height}
                <span class="chip">{e.width}×{e.height}</span>
              {/if}
            </div>
            <div class="row-preview">
              {#if e.kind_tag === 'text'}
                {e.preview}
              {:else}
                {e.mime ?? 'image'}
              {/if}
            </div>
          </div>
          <div class="row-actions">
            <button class="btn-icon" title="Send via plugin" disabled={resendingId === e.id} on:click={() => resend(e)}>📤</button>
            <button class="btn-icon" title="Copy to clipboard" on:click={() => copyToClipboard(e)}>📋</button>
            <button class="btn-icon" title={e.pinned ? 'Unpin' : 'Pin'} on:click={() => togglePin(e)}>{e.pinned ? '📌' : '📍'}</button>
            <button class="btn-icon btn-icon-danger" title="Delete" on:click={() => deleteEntry(e)}>✕</button>
          </div>
        </div>
      {/each}
    {/if}
  </div>
</div>

<style>
  .history-container {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
    background: var(--bg-primary);
  }

  .drag-strip {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-xs) var(--space-sm);
    background: var(--bg-tertiary);
    border-bottom: 1px solid var(--border-primary);
    flex-shrink: 0;
  }

  .drag-label {
    font-family: var(--font-gaming);
    font-size: 0.7rem;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }

  .drag-strip-actions {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
  }

  .stats-chip {
    font-family: var(--font-mono);
    font-size: 0.65rem;
    color: var(--text-muted);
  }

  .btn-strip {
    background: none;
    border: 1px solid var(--border-primary);
    color: var(--text-secondary);
    font-family: var(--font-gaming);
    font-size: 0.6rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 2px 8px;
    cursor: pointer;
    border-radius: var(--radius-sm);
    transition: all var(--transition-normal);
  }

  .btn-strip:hover {
    color: var(--accent-primary);
    border-color: var(--accent-primary);
  }

  .btn-strip-close:hover {
    color: var(--accent-danger);
    border-color: var(--accent-danger);
  }

  .filter-bar {
    display: flex;
    gap: var(--space-sm);
    padding: var(--space-sm);
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border-primary);
    align-items: center;
    flex-shrink: 0;
  }

  .search-input {
    flex: 1;
    max-width: 320px;
  }

  .filter-buttons {
    display: flex;
    gap: 4px;
  }

  .filter-btn {
    background: var(--bg-tertiary);
    border: 1px solid var(--border-primary);
    color: var(--text-secondary);
    font-family: var(--font-gaming);
    font-size: 0.6rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 4px 10px;
    cursor: pointer;
    border-radius: var(--radius-sm);
    transition: all var(--transition-normal);
  }

  .filter-btn:hover {
    color: var(--text-primary);
    border-color: var(--accent-primary);
  }

  .filter-active {
    color: var(--accent-primary);
    border-color: var(--accent-primary);
    background: rgba(0, 212, 255, 0.08);
  }

  .error-banner {
    background: rgba(255, 80, 80, 0.12);
    color: var(--accent-danger);
    padding: var(--space-xs) var(--space-sm);
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-bottom: 1px solid var(--accent-danger);
    font-family: var(--font-mono);
    font-size: 0.7rem;
  }

  .history-list {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-sm);
  }

  .row {
    display: flex;
    gap: var(--space-sm);
    padding: var(--space-sm);
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-md);
    margin-bottom: var(--space-xs);
    background: var(--bg-secondary);
    transition: border-color var(--transition-normal);
  }

  .row:hover {
    border-color: var(--accent-primary);
  }

  .row-pinned {
    border-left: 3px solid var(--accent-warning);
  }

  .row-selected {
    border-color: var(--accent-primary, #4ea1ff);
    background: var(--bg-selected, rgba(78, 161, 255, 0.08));
  }

  .row-thumb {
    width: 56px;
    height: 56px;
    border-radius: var(--radius-sm);
    background: var(--bg-tertiary);
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    flex-shrink: 0;
  }

  .row-thumb img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }

  .thumb-placeholder {
    font-size: 1.5rem;
    color: var(--text-muted);
  }

  .row-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .row-meta {
    display: flex;
    gap: var(--space-xs);
    align-items: center;
    flex-wrap: wrap;
  }

  .row-time,
  .row-size {
    font-family: var(--font-mono);
    font-size: 0.65rem;
    color: var(--text-muted);
  }

  .chip {
    font-family: var(--font-gaming);
    font-size: 0.55rem;
    color: var(--text-muted);
    background: var(--bg-tertiary);
    padding: 1px 6px;
    border-radius: var(--radius-sm);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .chip-pattern {
    color: var(--accent-primary);
    background: rgba(0, 212, 255, 0.08);
  }

  .chip-target {
    color: var(--accent-success, #4ade80);
    background: rgba(74, 222, 128, 0.08);
  }

  .row-preview {
    font-family: var(--font-mono);
    font-size: 0.75rem;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-actions {
    display: flex;
    gap: 4px;
    align-items: flex-start;
    flex-shrink: 0;
  }

  .btn-icon {
    background: none;
    border: 1px solid var(--border-primary);
    color: var(--text-secondary);
    width: 28px;
    height: 28px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: 0.85rem;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all var(--transition-normal);
  }

  .btn-icon:hover {
    color: var(--accent-primary);
    border-color: var(--accent-primary);
  }

  .btn-icon-danger:hover {
    color: var(--accent-danger);
    border-color: var(--accent-danger);
  }

  .btn-icon:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .empty-state {
    text-align: center;
    padding: var(--space-lg);
    color: var(--text-muted);
    font-family: var(--font-gaming);
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
</style>
