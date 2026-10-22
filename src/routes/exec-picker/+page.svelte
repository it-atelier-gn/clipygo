<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import { browser } from '$app/env';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { readText } from 'tauri-plugin-clipboard-api';

  interface ExecCommand {
    id: string;
    name: string;
    enabled: boolean;
    pattern: string;
    path: string;
    args: string;
    working_dir: string;
    pipe_stdin: boolean;
  }

  let clipboard = '';
  let commands: ExecCommand[] = [];
  let matched: Record<string, boolean> = {};
  let query = '';
  let highlightIndex = 0;
  let errorMsg = '';
  let ranName = '';
  let searchEl: HTMLInputElement;

  $: ordered = [...commands].sort((a, b) => {
    const am = matched[a.id] ? 0 : 1;
    const bm = matched[b.id] ? 0 : 1;
    return am - bm;
  });

  $: visible = ordered.filter((c) => {
    const q = query.trim().toLowerCase();
    return (
      q === '' ||
      c.name.toLowerCase().includes(q) ||
      c.path.toLowerCase().includes(q)
    );
  });

  async function load() {
    try {
      clipboard = await readText();
    } catch {
      clipboard = '';
    }
    try {
      const settings = await invoke<{ exec_commands: ExecCommand[] }>('get_settings');
      commands = (settings.exec_commands ?? []).filter((c) => c.enabled);
    } catch (e) {
      errorMsg = `Failed to load commands: ${e}`;
      commands = [];
    }
    try {
      const results = await invoke<boolean[]>('exec_match_commands', { commands, clipboard });
      matched = {};
      commands.forEach((c, i) => (matched[c.id] = results[i] ?? false));
    } catch {
      matched = {};
    }
    highlightIndex = 0;
  }

  function clampHighlight() {
    if (highlightIndex >= visible.length) highlightIndex = visible.length - 1;
    if (highlightIndex < 0) highlightIndex = 0;
  }

  async function scrollToHighlight() {
    await tick();
    document.querySelector('.cmd.highlight')?.scrollIntoView({ block: 'nearest' });
  }

  async function run() {
    clampHighlight();
    const cmd = visible[highlightIndex];
    if (!cmd) return;
    try {
      await invoke('exec_run', { command: cmd, clipboard });
      ranName = cmd.name;
      setTimeout(() => getCurrentWebviewWindow().hide(), 400);
    } catch (e) {
      errorMsg = `Launch failed: ${e}`;
    }
  }

  function close() {
    getCurrentWebviewWindow().hide();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      close();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      highlightIndex = Math.min(visible.length - 1, highlightIndex + 1);
      scrollToHighlight();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      highlightIndex = Math.max(0, highlightIndex - 1);
      scrollToHighlight();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      run();
    } else if (document.activeElement !== searchEl && !e.ctrlKey && !e.metaKey && !e.altKey) {
      if (e.key.length === 1) {
        e.preventDefault();
        query += e.key;
        highlightIndex = 0;
        searchEl?.focus();
      } else if (e.key === 'Backspace') {
        e.preventDefault();
        query = query.slice(0, -1);
        highlightIndex = 0;
        searchEl?.focus();
      }
    }
  }

  function truncate(text: string, max: number): string {
    return text.length > max ? text.slice(0, max) + '…' : text;
  }

  async function onShow() {
    query = '';
    errorMsg = '';
    ranName = '';
    await load();
    await tick();
    document.querySelector('.body')?.scrollTo(0, 0);
    searchEl?.focus();
  }

  let unlistenFocus: (() => void) | undefined;
  let unlistenShown: (() => void) | undefined;

  onMount(() => {
    if (!browser) return;
    const win = getCurrentWebviewWindow();
    win
      .onFocusChanged(({ payload: focused }) => {
        if (focused) searchEl?.focus();
      })
      .then((fn) => (unlistenFocus = fn));
    win.listen('window-shown', () => onShow()).then((fn) => (unlistenShown = fn));
    onShow();
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  onDestroy(() => {
    if (unlistenFocus) unlistenFocus();
    if (unlistenShown) unlistenShown();
  });
</script>

<div class="picker">
  <div class="drag-strip" data-tauri-drag-region>
    <span class="title" data-tauri-drag-region>Execute</span>
    <button class="btn-close" data-tauri-drag-region="false" on:click={close}>✕</button>
  </div>

  <div class="body">
    <div class="clip-line">
      <span class="clip-label">Clipboard</span>
      <span class="clip-value">{clipboard ? truncate(clipboard, 90) : '(empty or not text)'}</span>
    </div>

    <input
      class="search"
      bind:this={searchEl}
      bind:value={query}
      placeholder="Type to filter · ↑↓ to navigate · Enter to run"
      spellcheck="false"
      autocomplete="off"
    />

    <div class="cmd-list">
      {#each visible as cmd, i (cmd.id)}
        <button
          class="cmd"
          class:highlight={i === highlightIndex}
          on:click={() => { highlightIndex = i; run(); }}
        >
          <div class="cmd-head">
            <span class="cmd-name">{cmd.name || '(unnamed)'}</span>
            {#if matched[cmd.id] && cmd.pattern.trim() !== ''}
              <span class="cmd-match">matches</span>
            {/if}
          </div>
          <span class="cmd-path">{cmd.path} {cmd.args}</span>
        </button>
      {/each}

      {#if visible.length === 0}
        <div class="empty">
          {commands.length === 0 ? 'No commands yet. Add them in Settings.' : `No command matches "${query}"`}
        </div>
      {/if}
    </div>

    {#if errorMsg}<div class="error">{errorMsg}</div>{/if}
  </div>

  <div class="footer">
    <span class="hint">↑↓ navigate · Enter run · Esc close</span>
    {#if ranName}<span class="ran">Launched {ranName}</span>{/if}
  </div>
</div>

<style>
  .picker {
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

  .title {
    font-family: var(--font-serif);
    font-size: 1rem;
    font-weight: 500;
    color: var(--text-primary);
  }

  .btn-close {
    width: 24px;
    height: 24px;
    background: none;
    border: none;
    border-radius: var(--radius-md);
    color: var(--text-muted);
    cursor: pointer;
    font-size: 0.8rem;
    transition: background var(--transition-fast), color var(--transition-fast);
  }

  .btn-close:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .body {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-md) var(--space-lg);
    display: flex;
    flex-direction: column;
    gap: var(--space-md);
  }

  .search {
    width: 100%;
    height: 30px;
    padding: 0 var(--space-md);
    font-size: 0.85rem;
    color: var(--text-primary);
    background: var(--bg-elevated);
    border: 1px solid var(--border-secondary);
    border-radius: var(--radius-md);
    outline: none;
    transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
  }

  .search:focus {
    border-color: var(--accent-primary);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  .search::placeholder { color: var(--text-muted); }

  .error {
    font-size: 0.78rem;
    color: var(--accent-danger);
  }

  .footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 34px;
    padding: 0 var(--space-lg);
    border-top: 1px solid var(--border-primary);
    background: var(--bg-secondary);
    flex-shrink: 0;
  }

  .hint {
    font-size: 0.72rem;
    color: var(--text-muted);
  }

  .clip-line {
    display: flex;
    align-items: baseline;
    gap: var(--space-md);
    overflow: hidden;
  }

  .clip-label {
    font-size: 0.72rem;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .clip-value {
    font-family: var(--font-mono);
    font-size: 0.75rem;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .cmd-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0 calc(-1 * var(--space-md));
  }

  .cmd {
    display: flex;
    flex-direction: column;
    gap: 1px;
    text-align: left;
    padding: var(--space-sm) var(--space-md);
    border-radius: var(--radius-md);
    border: 1px solid transparent;
    background: transparent;
    color: inherit;
    cursor: pointer;
    transition: background var(--transition-fast);
  }

  .cmd:hover { background: var(--bg-hover); }

  .cmd.highlight {
    background: var(--accent-soft);
    border-color: var(--accent-ring);
  }

  .cmd-head {
    display: flex;
    align-items: center;
    gap: var(--space-md);
  }

  .cmd-name {
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .cmd-match {
    font-size: 0.66rem;
    line-height: 15px;
    color: var(--accent-primary);
    background: var(--accent-soft);
    border-radius: var(--radius-sm);
    padding: 0 5px;
  }

  .cmd-path {
    font-family: var(--font-mono);
    font-size: 0.7rem;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .empty {
    font-size: 0.8rem;
    color: var(--text-muted);
    padding: var(--space-xl);
    text-align: center;
  }

  .ran {
    font-size: 0.72rem;
    color: var(--accent-success);
  }
</style>
