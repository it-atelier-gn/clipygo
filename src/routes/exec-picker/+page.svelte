<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import { browser } from '$app/environment';
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
      // Route typing into the filter box even if it lost focus (e.g. window reshown).
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
    await load();
    await tick();
    searchEl?.focus();
  }

  let unlistenFocus: (() => void) | undefined;

  onMount(() => {
    if (!browser) return;
    getCurrentWebviewWindow()
      .onFocusChanged(({ payload: focused }) => {
        if (focused) onShow();
      })
      .then((fn) => (unlistenFocus = fn));
    onShow();
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  onDestroy(() => {
    if (unlistenFocus) unlistenFocus();
  });
</script>

<div class="picker">
  <div class="drag-strip" data-tauri-drag-region>
    <span class="title" data-tauri-drag-region>Execute · run a command</span>
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
          {commands.length === 0 ? 'No commands configured — add some in Settings.' : `No command matches "${query}"`}
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
    background: var(--bg-primary, var(--bg-surface));
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

  .title {
    font-family: var(--font-gaming);
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-secondary);
  }

  .btn-close {
    background: none;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 0.8rem;
  }
  .btn-close:hover { color: var(--accent-secondary); }

  .body {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-sm);
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .clip-line {
    display: flex;
    align-items: baseline;
    gap: var(--space-sm);
    overflow: hidden;
  }

  .clip-label {
    font-family: var(--font-gaming);
    font-size: 0.55rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .clip-value {
    font-family: var(--font-mono);
    font-size: 0.72rem;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .search {
    width: 100%;
    box-sizing: border-box;
    padding: var(--space-xs) var(--space-sm);
    font-family: var(--font-mono);
    font-size: 0.8rem;
    color: var(--text-primary);
    background: var(--bg-elevated);
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-md);
    outline: none;
    transition: border-color var(--transition-normal), box-shadow var(--transition-normal);
  }

  .search:focus {
    border-color: var(--accent-primary);
    box-shadow: var(--glow-primary);
  }

  .search::placeholder { color: var(--text-muted); }

  .cmd-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-xs);
  }

  .cmd {
    display: flex;
    flex-direction: column;
    gap: 2px;
    text-align: left;
    padding: var(--space-xs) var(--space-sm);
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-primary);
    background: var(--bg-elevated);
    cursor: pointer;
    transition: all var(--transition-normal);
  }

  .cmd:hover { border-color: var(--accent-primary); }

  .cmd.highlight {
    border-color: var(--accent-primary);
    background: var(--bg-surface);
    outline: 1px solid var(--accent-primary);
    outline-offset: 1px;
  }

  .cmd-head {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
  }

  .cmd-name {
    font-family: var(--font-gaming);
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .cmd-match {
    font-family: var(--font-gaming);
    font-size: 0.5rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--accent-primary);
    border: 1px solid var(--accent-primary);
    border-radius: var(--radius-sm);
    padding: 1px 4px;
  }

  .cmd-path {
    font-family: var(--font-mono);
    font-size: 0.68rem;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .empty {
    font-family: var(--font-mono);
    font-size: 0.72rem;
    color: var(--text-muted);
    padding: var(--space-md);
    text-align: center;
  }

  .error {
    font-family: var(--font-mono);
    font-size: 0.7rem;
    color: var(--accent-secondary);
    padding: var(--space-xs) var(--space-sm);
  }

  .footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-sm);
    border-top: 1px solid var(--border-primary);
    background: var(--bg-tertiary);
    flex-shrink: 0;
  }

  .hint {
    font-family: var(--font-mono);
    font-size: 0.65rem;
    color: var(--text-muted);
  }

  .ran {
    font-family: var(--font-gaming);
    font-size: 0.65rem;
    color: var(--accent-primary);
  }
</style>
