<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { invoke } from '@tauri-apps/api/core';
  import { onDestroy, onMount, tick } from 'svelte';
  import { browser } from '$app/env';
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

    try {
      const pending = await invoke<DebugLogEntry[]>('get_pending_debug_logs');
      for (const entry of pending) {
        addEntry(entry);
      }
    } catch (e) {
      console.error('Failed to get pending debug logs:', e);
    }

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
    <span class="drag-label" data-tauri-drag-region>Debug log</span>
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
    background: var(--bg-primary);
  }

  .drag-strip {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 36px;
    padding: 0 var(--space-sm) 0 var(--space-lg);
    border-bottom: 1px solid var(--border-primary);
    user-select: none;
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
    gap: var(--space-xs);
  }

  .btn-strip {
    height: 24px;
    min-width: 24px;
    padding: 0 var(--space-md);
    background: none;
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

  .btn-strip-active {
    color: var(--accent-primary);
  }

  .btn-strip-close {
    padding: 0;
  }

  .tab-bar {
    display: flex;
    gap: var(--space-xs);
    padding: 0 var(--space-md);
    border-bottom: 1px solid var(--border-primary);
    flex-shrink: 0;
    overflow-x: auto;
  }

  .tab {
    height: 30px;
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    padding: 0 var(--space-md);
    font-size: 0.78rem;
    color: var(--text-muted);
    cursor: pointer;
    transition: color var(--transition-fast), border-color var(--transition-fast);
    white-space: nowrap;
    display: flex;
    align-items: center;
    gap: var(--space-sm);
  }

  .tab:hover {
    color: var(--text-primary);
  }

  .tab-active {
    color: var(--text-primary);
    border-bottom-color: var(--accent-primary);
  }

  .tab-count {
    font-size: 0.66rem;
    line-height: 15px;
    color: var(--text-muted);
    background: var(--bg-tertiary);
    padding: 0 5px;
    border-radius: 8px;
  }

  .tab-active .tab-count {
    color: var(--accent-primary);
    background: var(--accent-soft);
  }

  .log-area {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-sm);
    background: var(--bg-secondary);
    font-family: var(--font-mono);
    font-size: 0.72rem;
    line-height: 1.5;
  }

  .log-line {
    display: flex;
    gap: var(--space-md);
    padding: 0 var(--space-sm);
    border-radius: var(--radius-sm);
  }

  .log-line:hover {
    background: var(--bg-hover);
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

  .log-error .log-level,
  .log-error .log-message { color: var(--accent-danger); }

  .log-warn .log-level,
  .log-warn .log-message { color: var(--accent-warning); }

  .log-info .log-level { color: var(--accent-primary); }

  .log-debug .log-level { color: var(--text-muted); }

  .empty-state {
    text-align: center;
    padding: var(--space-2xl);
    color: var(--text-muted);
    font-family: var(--font-primary);
    font-size: 0.85rem;
  }
</style>
