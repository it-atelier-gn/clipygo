<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { invoke } from '@tauri-apps/api/core';
  import { onDestroy, onMount, tick } from 'svelte';
  import { browser } from '$app/environment';
  import type { UnlistenFn } from '@tauri-apps/api/event';

  interface DebugLogEntry {
    source: string;
    message: string;
    timestamp: number;
    level: string;
  }

  const MAX_LINES = 2000;

  let sources: string[] = ['app'];
  let activeTab = 'app';
  let autoScroll = true;
  let unlistenDebugLog: UnlistenFn;

  // Per-source log buffers
  let logs: Record<string, DebugLogEntry[]> = { app: [] };

  let logContainer: HTMLElement;

  function addEntry(entry: DebugLogEntry) {
    const src = entry.source;
    if (!logs[src]) {
      logs[src] = [];
      if (!sources.includes(src)) {
        sources = [...sources, src];
      }
    }
    logs[src] = [...logs[src].slice(-(MAX_LINES - 1)), entry];
    logs = logs; // trigger reactivity

    if (autoScroll && src === activeTab) {
      tick().then(scrollToBottom);
    }
  }

  function scrollToBottom() {
    if (logContainer) {
      logContainer.scrollTop = logContainer.scrollHeight;
    }
  }

  function clearTab() {
    logs[activeTab] = [];
    logs = logs;
  }

  function formatTime(ts: number): string {
    const d = new Date(ts * 1000);
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
  }

  function levelClass(level: string): string {
    switch (level) {
      case 'error': return 'log-error';
      case 'warn': return 'log-warn';
      case 'info': return 'log-info';
      default: return 'log-debug';
    }
  }

  onMount(async () => {
    if (!browser) return;

    // Fetch known plugin sources
    try {
      const result = await invoke<string[]>('get_debug_sources');
      for (const src of result) {
        if (!logs[src]) {
          logs[src] = [];
        }
        if (!sources.includes(src)) {
          sources = [...sources, src];
        }
      }
    } catch (e) {
      console.error('Failed to get debug sources:', e);
    }

    // Drain pending logs
    try {
      const pending = await invoke<DebugLogEntry[]>('get_pending_debug_logs');
      for (const entry of pending) {
        addEntry(entry);
      }
    } catch (e) {
      console.error('Failed to get pending debug logs:', e);
    }

    // Listen for new debug log entries
    unlistenDebugLog = await listen<DebugLogEntry>('debug-log', (event) => {
      addEntry(event.payload);
    });
  });

  onDestroy(() => {
    if (!browser) return;
    if (unlistenDebugLog) unlistenDebugLog();
  });
</script>

<div class="debug-container">
  <div class="drag-strip" data-tauri-drag-region>
    <span class="drag-label" data-tauri-drag-region>debug log</span>
    <div class="drag-strip-actions" data-tauri-drag-region>
      <button class="btn-strip" data-tauri-drag-region="false" on:click={clearTab}>
        Clear
      </button>
      <button
        class="btn-strip"
        class:btn-strip-active={autoScroll}
        data-tauri-drag-region="false"
        on:click={() => { autoScroll = !autoScroll; if (autoScroll) scrollToBottom(); }}
      >
        Auto-scroll {autoScroll ? 'on' : 'off'}
      </button>
      <button class="btn-strip btn-strip-close" data-tauri-drag-region="false" on:click={() => getCurrentWebviewWindow().hide()}>
        ✕
      </button>
    </div>
  </div>

  <div class="tab-bar">
    {#each sources as src}
      <button
        class="tab"
        class:tab-active={activeTab === src}
        on:click={() => { activeTab = src; if (autoScroll) tick().then(scrollToBottom); }}
      >
        {src}
        {#if (logs[src]?.length ?? 0) > 0}
          <span class="tab-count">{logs[src].length}</span>
        {/if}
      </button>
    {/each}
  </div>

  <div class="log-area" bind:this={logContainer}>
    {#if logs[activeTab]?.length > 0}
      {#each logs[activeTab] as entry}
        <div class="log-line {levelClass(entry.level)}">
          <span class="log-time">{formatTime(entry.timestamp)}</span>
          <span class="log-level">{entry.level.toUpperCase().padEnd(5)}</span>
          <span class="log-message">{entry.message}</span>
        </div>
      {/each}
    {:else}
      <div class="empty-state">No log entries</div>
    {/if}
  </div>
</div>

<style>
  .debug-container {
    display: flex;
    flex-direction: column;
    height: 100vh;
    overflow: hidden;
  }

  .drag-strip {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-xs) var(--space-sm);
    background: var(--bg-tertiary);
    border-bottom: 1px solid var(--border-primary);
    cursor: grab;
    user-select: none;
    flex-shrink: 0;
  }

  .drag-label {
    font-family: var(--font-gaming);
    font-size: 0.6rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
  }

  .drag-strip-actions {
    display: flex;
    align-items: center;
    gap: var(--space-xs);
  }

  .btn-strip {
    background: none;
    border: none;
    color: var(--text-muted);
    font-family: var(--font-gaming);
    font-size: 0.6rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    cursor: pointer;
    padding: 2px var(--space-xs);
    border-radius: var(--radius-sm);
    transition: color var(--transition-normal);
  }

  .btn-strip:hover {
    color: var(--accent-primary);
  }

  .btn-strip-active {
    color: var(--accent-primary);
  }

  .btn-strip-close:hover {
    color: var(--accent-secondary);
  }

  /* Tab bar */
  .tab-bar {
    display: flex;
    gap: 0;
    background: var(--bg-secondary);
    border-bottom: 2px solid var(--border-primary);
    flex-shrink: 0;
    overflow-x: auto;
  }

  .tab {
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    margin-bottom: -2px;
    padding: var(--space-xs) var(--space-md);
    font-family: var(--font-gaming);
    font-size: 0.65rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
    cursor: pointer;
    transition: all var(--transition-normal);
    white-space: nowrap;
    display: flex;
    align-items: center;
    gap: var(--space-xs);
  }

  .tab:hover {
    color: var(--text-secondary);
    background: var(--bg-tertiary);
  }

  .tab-active {
    color: var(--accent-primary);
    border-bottom-color: var(--accent-primary);
  }

  .tab-count {
    font-family: var(--font-mono);
    font-size: 0.55rem;
    color: var(--text-muted);
    background: var(--bg-tertiary);
    padding: 1px 4px;
    border-radius: var(--radius-sm);
  }

  .tab-active .tab-count {
    color: var(--accent-primary);
    background: rgba(0, 212, 255, 0.1);
  }

  /* Log area */
  .log-area {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-xs);
    background: var(--bg-primary);
    font-family: var(--font-mono);
    font-size: 0.75rem;
    line-height: 1.5;
  }

  .log-line {
    display: flex;
    gap: var(--space-sm);
    padding: 1px var(--space-xs);
    border-radius: 2px;
  }

  .log-line:hover {
    background: var(--bg-tertiary);
  }

  .log-time {
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .log-level {
    flex-shrink: 0;
    width: 3.5em;
  }

  .log-message {
    color: var(--text-secondary);
    white-space: pre-wrap;
    word-break: break-word;
  }

  /* Level colors */
  .log-error .log-level { color: var(--accent-danger); }
  .log-error .log-message { color: var(--accent-danger); }

  .log-warn .log-level { color: var(--accent-warning); }
  .log-warn .log-message { color: var(--accent-warning); }

  .log-info .log-level { color: var(--accent-primary); }

  .log-debug .log-level { color: var(--text-muted); }

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
