<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { browser } from '$app/env';
  import { invoke } from '@tauri-apps/api/core';
  import { emitTo, listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { getCurrentWebviewWindow, WebviewWindow } from '@tauri-apps/api/webviewWindow';

  type KindTag = 'text' | 'image' | 'html' | 'rtf' | 'files';

  interface HistoryEntryView {
    id: string;
    timestamp: number;
    kind_tag: KindTag;
    preview: string;
    line_count: number;
    mime?: string;
    width?: number;
    height?: number;
    size_bytes: number;
    matched_pattern?: string;
    pinned: boolean;
    last_sent_to?: string;
    formats: string[];
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
  type FilterKind = 'all' | 'text' | 'html' | 'rtf' | 'image' | 'files';
  let filterKind: FilterKind = 'all';
  let pinnedOnly = false;
  let query = '';
  let thumbnails: Record<string, string> = {};
  let unlistenChanged: UnlistenFn | null = null;
  let unlistenFocus: UnlistenFn | null = null;
  let unlistenShown: UnlistenFn | null = null;
  let resendingId: string | null = null;
  let errorMsg = '';
  let selectedIndex = 0;
  let searchInput: HTMLInputElement | null = null;
  let rowEls: Record<string, HTMLDivElement> = {};
  let listEl: HTMLDivElement | null = null;

  async function resetWindowState() {
    query = '';
    filterKind = 'all';
    pinnedOnly = false;
    selectedIndex = 0;
    errorMsg = '';
    await refresh();
    listEl?.scrollTo(0, 0);
    searchInput?.focus();
  }

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

  interface ResendPayload {
    kind: string;
    text?: string;
    html?: string;
    rtf?: string;
    files?: string[];
    image_base64?: string;
    mime?: string;
  }

  async function writePayloadToClipboard(payload: ResendPayload) {
    await invoke('clipboard_write_payload', { payload });
  }

  async function copyToClipboard(e: HistoryEntryView) {
    try {
      const payload = await invoke<ResendPayload>('history_resend', { id: e.id });
      await writePayloadToClipboard(payload);
    } catch (err) {
      errorMsg = `Copy failed: ${err}`;
    }
  }

  async function resend(e: HistoryEntryView) {
    resendingId = e.id;
    try {
      const payload = await invoke<ResendPayload>('history_resend', { id: e.id });
      await writePayloadToClipboard(payload);
      const main = await WebviewWindow.getByLabel('main');
      if (main) {
        if (!(await main.isVisible())) {
          await emitTo('main', 'window-shown');
        }
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
    unlistenShown = await win.listen('window-shown', () => {
      resetWindowState();
    });
  });

  onDestroy(() => {
    if (unlistenChanged) unlistenChanged();
    if (unlistenFocus) unlistenFocus();
    if (unlistenShown) unlistenShown();
  });
</script>

<svelte:window on:keydown={onKeyDown} />

<div class="history-container">
  <div class="drag-strip" data-tauri-drag-region>
    <span class="drag-label" data-tauri-drag-region>History</span>
    <div class="drag-strip-actions" data-tauri-drag-region>
      {#if stats}
        <span class="stats-chip" data-tauri-drag-region>
          {stats.items} items · {formatBytes(stats.bytes_used)} / {formatBytes(stats.bytes_cap)}
          {stats.persisted_to_disk ? '· disk' : '· memory'}
        </span>
      {/if}
      <button class="btn-strip" data-tauri-drag-region="false" on:click={() => clearAll(false)}>Clear</button>
      <button class="btn-strip btn-strip-close" data-tauri-drag-region="false" aria-label="Hide" on:click={() => getCurrentWebviewWindow().hide()}>✕</button>
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
      <button class="filter-btn" class:filter-active={filterKind === 'html'} on:click={() => { filterKind = 'html'; selectedIndex = 0; refresh(); }}>HTML</button>
      <button class="filter-btn" class:filter-active={filterKind === 'rtf'} on:click={() => { filterKind = 'rtf'; selectedIndex = 0; refresh(); }}>RTF</button>
      <button class="filter-btn" class:filter-active={filterKind === 'image'} on:click={() => { filterKind = 'image'; selectedIndex = 0; refresh(); }}>Images</button>
      <button class="filter-btn" class:filter-active={filterKind === 'files'} on:click={() => { filterKind = 'files'; selectedIndex = 0; refresh(); }}>Files</button>
      <button class="filter-btn" class:filter-active={pinnedOnly} on:click={() => { pinnedOnly = !pinnedOnly; selectedIndex = 0; refresh(); }}>📌 Pinned</button>
    </div>
  </div>

  {#if errorMsg}
    <div class="error-banner">{errorMsg} <button class="btn-strip" on:click={() => (errorMsg = '')}>✕</button></div>
  {/if}

  <div class="history-list" bind:this={listEl}>
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
            {:else if e.kind_tag === 'html'}
              <div class="thumb-placeholder">🌐</div>
            {:else if e.kind_tag === 'rtf'}
              <div class="thumb-placeholder">📝</div>
            {:else if e.kind_tag === 'files'}
              <div class="thumb-placeholder">📁</div>
            {:else}
              <div class="thumb-placeholder">📋</div>
            {/if}
          </div>
          <div class="row-body">
            <div class="row-meta">
              <span class="row-time">{formatTime(e.timestamp)}</span>
              <span class="row-size">{formatBytes(e.size_bytes)}</span>
              {#each e.formats as f}
                <span class="chip chip-kind">{f}</span>
              {/each}
              {#if e.kind_tag === 'files' && e.line_count > 0}
                <span class="chip">{e.line_count} {e.line_count === 1 ? 'file' : 'files'}</span>
              {:else if e.line_count > 1}
                <span class="chip chip-lines">{e.line_count} lines</span>
              {/if}
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
            <div class="row-preview" class:row-preview-multiline={e.line_count > 1}>
              {#if e.kind_tag === 'image'}
                {e.mime ?? 'image'}
              {:else}
                {e.preview}
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
    height: 36px;
    padding: 0 var(--space-sm) 0 var(--space-lg);
    border-bottom: 1px solid var(--border-primary);
    flex-shrink: 0;
  }

  .drag-label {
    font-family: var(--font-serif);
    font-size: 1rem;
    font-weight: 500;
    color: var(--text-primary);
  }

  .drag-strip-actions {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
  }

  .stats-chip {
    font-size: 0.72rem;
    color: var(--text-muted);
    margin-right: var(--space-sm);
  }

  .btn-strip {
    height: 24px;
    min-width: 24px;
    padding: 0 var(--space-md);
    background: transparent;
    border: none;
    border-radius: var(--radius-md);
    color: var(--text-muted);
    font-size: 0.78rem;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }

  .btn-strip:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .btn-strip-close {
    padding: 0;
  }

  .filter-bar {
    display: flex;
    gap: var(--space-md);
    padding: var(--space-md) var(--space-lg);
    border-bottom: 1px solid var(--border-primary);
    align-items: center;
    flex-shrink: 0;
  }

  .search-input {
    flex: 1;
    max-width: 280px;
  }

  .filter-buttons {
    display: flex;
    gap: 2px;
    padding: 2px;
    background: var(--bg-tertiary);
    border-radius: var(--radius-md);
  }

  .filter-btn {
    height: 22px;
    padding: 0 var(--space-md);
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    font-size: 0.75rem;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }

  .filter-btn:hover {
    color: var(--text-primary);
  }

  .filter-active {
    background: var(--bg-surface);
    color: var(--text-primary);
    box-shadow: var(--shadow-sm);
  }

  .error-banner {
    background: color-mix(in srgb, var(--accent-danger) 8%, transparent);
    color: var(--accent-danger);
    padding: var(--space-sm) var(--space-lg);
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-bottom: 1px solid color-mix(in srgb, var(--accent-danger) 25%, transparent);
    font-size: 0.78rem;
  }

  .history-list {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-sm);
  }

  .row {
    display: flex;
    align-items: center;
    gap: var(--space-md);
    padding: var(--space-sm) var(--space-md);
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    transition: background var(--transition-fast);
  }

  .row:hover {
    background: var(--bg-hover);
  }

  .row-pinned .row-time::before {
    content: '';
    display: inline-block;
    width: 6px;
    height: 6px;
    margin-right: 5px;
    border-radius: 50%;
    background: var(--accent-primary);
    vertical-align: 1px;
  }

  .row-selected,
  .row-selected:hover {
    background: var(--accent-soft);
    border-color: var(--accent-ring);
  }

  .row-thumb {
    width: 36px;
    height: 36px;
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
    font-size: 1rem;
    opacity: 0.7;
  }

  .row-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .row-meta {
    display: flex;
    gap: var(--space-sm);
    align-items: center;
    flex-wrap: wrap;
  }

  .row-time,
  .row-size {
    font-size: 0.7rem;
    color: var(--text-muted);
  }

  .chip {
    font-size: 0.66rem;
    line-height: 15px;
    color: var(--text-muted);
    background: var(--bg-tertiary);
    padding: 0 5px;
    border-radius: var(--radius-sm);
  }

  .chip-pattern,
  .chip-kind {
    color: var(--accent-primary);
    background: var(--accent-soft);
  }

  .chip-lines {
    color: var(--text-secondary);
  }

  .chip-target {
    color: var(--accent-success);
    background: color-mix(in srgb, var(--accent-success) 12%, transparent);
  }

  .row-preview {
    font-family: var(--font-mono);
    font-size: 0.75rem;
    color: var(--text-primary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .row-preview-multiline {
    white-space: pre-wrap;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    word-break: break-word;
  }

  .row-actions {
    display: flex;
    gap: 2px;
    align-items: center;
    flex-shrink: 0;
    opacity: 0;
    transition: opacity var(--transition-fast);
  }

  .row:hover .row-actions,
  .row-selected .row-actions {
    opacity: 1;
  }

  .btn-icon {
    background: none;
    border: none;
    color: var(--text-muted);
    width: 24px;
    height: 24px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: 0.8rem;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background var(--transition-fast), color var(--transition-fast);
  }

  .btn-icon:hover {
    background: var(--bg-active);
    color: var(--text-primary);
  }

  .btn-icon-danger:hover {
    color: var(--accent-danger);
  }

  .btn-icon:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .empty-state {
    text-align: center;
    padding: var(--space-2xl);
    color: var(--text-muted);
    font-size: 0.85rem;
  }
</style>
