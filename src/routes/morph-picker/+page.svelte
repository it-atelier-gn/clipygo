<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { browser } from '$app/environment';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { readText, writeText } from 'tauri-plugin-clipboard-api';

  interface TransformDef {
    id: string;
    label: string;
    group: string;
  }

  const TRANSFORMS: TransformDef[] = [
    { id: 'strip_tracking', label: 'Strip URL tracking', group: 'Web' },
    { id: 'strip_html', label: 'Strip HTML tags', group: 'Web' },
    { id: 'html_encode', label: 'HTML encode', group: 'Web' },
    { id: 'json_pretty', label: 'JSON pretty', group: 'Format' },
    { id: 'json_minify', label: 'JSON minify', group: 'Format' },
    { id: 'xml_pretty', label: 'XML pretty', group: 'Format' },
    { id: 'base64_encode', label: 'Base64 encode', group: 'Encode' },
    { id: 'base64_decode', label: 'Base64 decode', group: 'Encode' },
    { id: 'url_encode', label: 'URL encode', group: 'Encode' },
    { id: 'url_decode', label: 'URL decode', group: 'Encode' },
    { id: 'uppercase', label: 'UPPERCASE', group: 'Case' },
    { id: 'lowercase', label: 'lowercase', group: 'Case' },
    { id: 'title_case', label: 'Title Case', group: 'Case' },
    { id: 'snake_case', label: 'snake_case', group: 'Case' },
    { id: 'camel_case', label: 'camelCase', group: 'Case' },
    { id: 'kebab_case', label: 'kebab-case', group: 'Case' },
    { id: 'slugify', label: 'Slugify', group: 'Text' },
    { id: 'remove_diacritics', label: 'Remove accents', group: 'Text' },
    { id: 'straighten_quotes', label: 'Straighten quotes/dashes', group: 'Text' },
    { id: 'trim', label: 'Trim whitespace', group: 'Lines' },
    { id: 'collapse_whitespace', label: 'Collapse whitespace', group: 'Lines' },
    { id: 'trim_lines', label: 'Trim each line', group: 'Lines' },
    { id: 'normalize_newlines', label: 'Normalize newlines', group: 'Lines' },
    { id: 'sort_lines', label: 'Sort lines', group: 'Lines' },
    { id: 'reverse_lines', label: 'Reverse lines', group: 'Lines' },
    { id: 'dedupe_lines', label: 'Dedupe lines', group: 'Lines' },
    { id: 'remove_empty_lines', label: 'Remove empty lines', group: 'Lines' },
  ];

  const GROUPS = ['Web', 'Format', 'Encode', 'Case', 'Text', 'Lines'];

  let clipboard = '';
  let selected: string | null = null;
  let preview = '';
  let applied = false;
  let errorMsg = '';
  let query = '';
  let highlightIndex = 0;
  let searchEl: HTMLInputElement;

  $: visible = TRANSFORMS.filter((t) => {
    const q = query.trim().toLowerCase();
    return q === '' || t.label.toLowerCase().includes(q) || t.id.includes(q);
  });

  async function loadClipboard() {
    try {
      clipboard = await readText();
    } catch {
      clipboard = '';
    }
    await previewHighlighted();
  }

  async function runPreview(id: string) {
    selected = id;
    applied = false;
    try {
      preview = await invoke<string>('morph_preview', { text: clipboard, transform: id });
      errorMsg = '';
    } catch (e) {
      errorMsg = `Preview failed: ${e}`;
      preview = '';
    }
  }

  async function previewHighlighted() {
    if (highlightIndex >= visible.length) highlightIndex = visible.length - 1;
    if (highlightIndex < 0) highlightIndex = 0;
    const t = visible[highlightIndex];
    if (!t) {
      selected = null;
      preview = '';
      return;
    }
    await runPreview(t.id);
    await tick();
    document.querySelector('.chip.highlight')?.scrollIntoView({ block: 'nearest' });
  }

  function onSearch() {
    highlightIndex = 0;
    previewHighlighted();
  }

  function selectChip(id: string) {
    const idx = visible.findIndex((t) => t.id === id);
    if (idx >= 0) {
      highlightIndex = idx;
      previewHighlighted();
    }
  }

  $: changed = selected !== null && preview !== clipboard;

  async function apply() {
    if (!changed) return;
    try {
      await writeText(preview);
      clipboard = preview;
      applied = true;
      setTimeout(() => getCurrentWebviewWindow().hide(), 500);
    } catch (e) {
      errorMsg = `Apply failed: ${e}`;
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
      previewHighlighted();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      highlightIndex = Math.max(0, highlightIndex - 1);
      previewHighlighted();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      if (changed) apply();
    }
  }

  onMount(() => {
    if (!browser) return;
    loadClipboard();
    searchEl?.focus();
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });
</script>

<div class="picker">
  <div class="drag-strip" data-tauri-drag-region>
    <span class="title" data-tauri-drag-region>Morph · transform clipboard</span>
    <button class="btn-close" data-tauri-drag-region="false" on:click={close}>✕</button>
  </div>

  <div class="body">
    <div class="panel source">
      <div class="panel-head">
        <span class="panel-label">Clipboard</span>
        <button class="btn-mini" on:click={loadClipboard}>Reload</button>
      </div>
      <pre class="text-box">{clipboard || '(clipboard is empty or not text)'}</pre>
    </div>

    <input
      class="search"
      bind:this={searchEl}
      bind:value={query}
      on:input={onSearch}
      placeholder="Type to filter · ↑↓ to navigate · Enter to apply"
      spellcheck="false"
      autocomplete="off"
    />

    <div class="transforms">
      {#each GROUPS as group}
        {@const groupItems = visible.filter(t => t.group === group)}
        {#if groupItems.length > 0}
          <div class="group">
            <span class="group-label">{group}</span>
            <div class="chips">
              {#each groupItems as t}
                <button
                  class="chip"
                  class:active={selected === t.id}
                  class:highlight={visible.indexOf(t) === highlightIndex}
                  on:click={() => selectChip(t.id)}
                >{t.label}</button>
              {/each}
            </div>
          </div>
        {/if}
      {/each}
      {#if visible.length === 0}
        <div class="no-match">No transformation matches "{query}"</div>
      {/if}
    </div>

    <div class="panel result">
      <div class="panel-head">
        <span class="panel-label">Result {selected ? `· ${TRANSFORMS.find(t => t.id === selected)?.label}` : ''}</span>
        {#if selected && !changed}<span class="nochange">no change</span>{/if}
      </div>
      <pre class="text-box">{selected ? preview : '(pick a transformation)'}</pre>
    </div>

    {#if errorMsg}<div class="error">{errorMsg}</div>{/if}
  </div>

  <div class="footer">
    <span class="hint">↑↓ navigate · Enter apply · Esc close</span>
    <button class="btn-apply" disabled={!changed} on:click={apply}>
      {applied ? 'Applied!' : 'Apply'}
    </button>
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

  .panel {
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    overflow: hidden;
  }

  .panel-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--space-xs) var(--space-sm);
    border-bottom: 1px solid var(--border-primary);
  }

  .panel-label {
    font-family: var(--font-gaming);
    font-size: 0.6rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .nochange {
    font-family: var(--font-mono);
    font-size: 0.6rem;
    color: var(--accent-secondary);
  }

  .text-box {
    margin: 0;
    padding: var(--space-sm);
    font-family: var(--font-mono);
    font-size: 0.75rem;
    line-height: 1.4;
    color: var(--text-secondary);
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 110px;
    overflow-y: auto;
  }

  .result .text-box { color: var(--text-primary); }

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

  .transforms {
    display: flex;
    flex-direction: column;
    gap: var(--space-xs);
  }

  .no-match {
    font-family: var(--font-mono);
    font-size: 0.7rem;
    color: var(--text-muted);
    padding: var(--space-sm);
    text-align: center;
  }

  .group { display: flex; flex-direction: column; gap: 4px; }

  .group-label {
    font-family: var(--font-gaming);
    font-size: 0.55rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
  }

  .chips { display: flex; flex-wrap: wrap; gap: var(--space-xs); }

  .chip {
    font-family: var(--font-mono);
    font-size: 0.7rem;
    padding: 3px var(--space-sm);
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-primary);
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    cursor: pointer;
    transition: all var(--transition-normal);
  }

  .chip:hover { border-color: var(--accent-primary); color: var(--accent-primary); }

  .chip.active {
    border-color: var(--accent-primary);
    color: var(--text-primary);
    background: var(--bg-surface);
    box-shadow: var(--glow-primary);
  }

  .chip.highlight {
    border-color: var(--accent-primary);
    color: var(--text-primary);
    background: var(--bg-surface);
    outline: 1px solid var(--accent-primary);
    outline-offset: 1px;
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

  .btn-apply {
    font-family: var(--font-gaming);
    font-size: 0.7rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: var(--space-xs) var(--space-lg);
    border-radius: var(--radius-sm);
    border: 1px solid var(--accent-primary);
    background: var(--bg-surface);
    color: var(--accent-primary);
    cursor: pointer;
    transition: all var(--transition-normal);
  }

  .btn-apply:hover:not(:disabled) {
    box-shadow: var(--glow-primary);
    color: var(--text-primary);
  }

  .btn-apply:disabled { opacity: 0.4; cursor: not-allowed; }
</style>
