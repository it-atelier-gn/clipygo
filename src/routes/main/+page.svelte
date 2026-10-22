<script lang="ts">
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { listen } from '@tauri-apps/api/event';
  import { onDestroy, onMount } from "svelte";
  import { invoke } from '@tauri-apps/api/core';
  import { browser } from '$app/env';
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import {
    hasText,
    readText,
    hasImage,
    readImageBase64,
    hasHTML,
    readHtml,
    hasFiles,
    readFiles,
  } from "tauri-plugin-clipboard-api";

  interface Target {
    id: string;
    provider: string;
    formats: string[];
    title: string;
    description: string;
    image: string;
  }

  interface SendPayload {
    content: string;
    format: string;
  }

  interface PluginError {
    plugin_name: string;
    message: string;
  }

  interface GetTargetsResult {
    targets: Target[];
    errors: PluginError[];
  }

  const FORMAT_PRIORITY = ['files', 'html', 'image', 'text'] as const;

  let unlistenWindowEvents: UnlistenFn;
  let unlistenClipboard: UnlistenFn;
  let unlistenShown: UnlistenFn;
  let clipboardContent = "";
  let clipboardHtml = "";
  let clipboardImage = "";
  let clipboardFiles: string[] = [];
  let targets: Target[] = [];
  let pluginErrors: PluginError[] = [];
  let loadingTargets = false;
  let sendingTo: string | null = null;
  let message = "";
  let messageType: 'success' | 'error' | '' = '';
  let selectedTargetIndex = 0;

  $: clipboardFormat = clipboardFiles.length ? 'files' : clipboardContent ? 'text' : clipboardImage ? 'image' : null;

  $: formatContent = {
    files: clipboardFiles.join('\n'),
    html: clipboardHtml,
    image: clipboardImage,
    text: clipboardContent,
  };
  $: availableFormats = FORMAT_PRIORITY.filter(f => formatContent[f].length > 0);

  function bestFormatFor(target: Target): typeof FORMAT_PRIORITY[number] | null {
    return FORMAT_PRIORITY.find(f => availableFormats.includes(f) && target.formats.includes(f)) ?? null;
  }

  $: compatibleCount = targets.filter(t => bestFormatFor(t) !== null).length;

  function isCompatible(target: Target): boolean {
    return bestFormatFor(target) !== null;
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      const window = getCurrentWebviewWindow();
      window.hide();
      return;
    }

    if (targets.length > 0) {
      if (event.key === 'ArrowDown') {
        event.preventDefault();
        selectedTargetIndex = (selectedTargetIndex + 1) % targets.length;
      } else if (event.key === 'ArrowUp') {
        event.preventDefault();
        selectedTargetIndex = selectedTargetIndex === 0 ? targets.length - 1 : selectedTargetIndex - 1;
      } else if (event.key === 'Enter') {
        event.preventDefault();
        if (targets[selectedTargetIndex]) {
          sendToTarget(targets[selectedTargetIndex]);
        }
      }
    }
  }

  function showMessage(text: string, type: 'success' | 'error') {
    message = text;
    messageType = type;
    setTimeout(() => {
      message = "";
      messageType = '';
    }, 3000);
  }

  function handleImageError(event: Event) {
    const img = event.target as HTMLImageElement;
    const fallback = img.nextElementSibling as HTMLElement;
    img.style.display = 'none';
    if (fallback) {
      fallback.style.display = 'flex';
    }
  }

  async function readClipboard() {
    try {
      if (await hasFiles()) {
        clipboardFiles = await readFiles();
        clipboardContent = "";
        clipboardHtml = "";
        clipboardImage = "";
      } else if (await hasText()) {
        clipboardContent = await readText();
        clipboardHtml = (await hasHTML()) ? await readHtml() : "";
        clipboardImage = "";
        clipboardFiles = [];
      } else if (await hasImage()) {
        clipboardImage = await readImageBase64();
        clipboardContent = "";
        clipboardHtml = "";
        clipboardFiles = [];
      } else {
        clipboardContent = "";
        clipboardHtml = "";
        clipboardImage = "";
        clipboardFiles = [];
      }
    } catch (e) {
      console.error('Failed to read clipboard:', e);
    }
  }

  function fileName(path: string): string {
    return path.replace(/[/\\]+$/, '').split(/[/\\]/).pop() ?? path;
  }

  async function loadTargets() {
    loadingTargets = true;
    try {
      const result: GetTargetsResult = await invoke('get_targets');
      targets = result.targets;
      pluginErrors = result.errors;
      selectedTargetIndex = 0;
    } catch (error) {
      console.error('Failed to load targets:', error);
      showMessage('Failed to load targets', 'error');
    } finally {
      loadingTargets = false;
    }
  }

  async function sendToTarget(target: Target, fromClick = false) {
    if (availableFormats.length === 0) {
      showMessage('No clipboard content to send', 'error');
      return;
    }

    const format = bestFormatFor(target);
    if (!format) {
      showMessage(`${target.title} does not support ${clipboardFormat ?? 'this'} content`, 'error');
      return;
    }

    if (fromClick) {
      selectedTargetIndex = targets.findIndex(t => t.id === target.id);
    }

    sendingTo = target.id;
    try {
      const payload: SendPayload = {
        content: formatContent[format],
        format
      };

      await invoke('send_to_target', { 
        targetId: target.id, 
        payload 
      });

      showMessage(`Content sent to ${target.title}`, 'success');

      setTimeout(() => {
        const window = getCurrentWebviewWindow();
        window.hide();
      }, 1500);

    } catch (error) {
      console.error('Failed to send to target:', error);
      const reason = typeof error === 'string' ? error : `Failed to send to ${target.title}`;
      showMessage(reason, 'error');
    } finally {
      sendingTo = null;
    }
  }
  
  onMount(async () => {
    if (!browser) return;

    document.addEventListener('keydown', handleKeydown);

    unlistenWindowEvents = await listen('tauri://focus', async () => {
      await readClipboard();
      await loadTargets();
    });

    unlistenShown = await getCurrentWebviewWindow().listen('window-shown', async () => {
      message = "";
      messageType = '';
      sendingTo = null;
      selectedTargetIndex = 0;
      await readClipboard();
      await loadTargets();
      document.querySelector('.content')?.scrollTo(0, 0);
    });

    unlistenClipboard = await listen('plugin:clipboard://clipboard-monitor/update', async () => {
      await readClipboard();
    });

    await readClipboard();
    await loadTargets();
  });

  onDestroy(() => {
    if (!browser) return;
    document.removeEventListener('keydown', handleKeydown);
    if (unlistenWindowEvents) unlistenWindowEvents();
    if (unlistenClipboard) unlistenClipboard();
    if (unlistenShown) unlistenShown();
  });

  $: if (targets.length > 0 && selectedTargetIndex >= targets.length) {
    selectedTargetIndex = 0;
  }
</script>

<div class="app" data-tauri-drag-region>
  <header class="header" data-tauri-drag-region>
    <div class="header-content flex justify-between items-center" data-tauri-drag-region>
      <h1 class="app-title" data-tauri-drag-region>clipygo</h1>
      <div class="header-actions flex gap-sm">
        <button
          class="btn btn-secondary btn-sm"
          on:click={async () => { await readClipboard(); await loadTargets(); }}
          disabled={loadingTargets}
        >
          <span class="icon" class:spinning={loadingTargets}>↻</span>
          Refresh
        </button>
        <button
          class="btn btn-ghost btn-icon"
          aria-label="Hide"
          on:click={() => getCurrentWebviewWindow().hide()}
        >
          ✕
        </button>
      </div>
    </div>
  </header>

  {#if message}
    <div class="message message-{messageType}" data-tauri-drag-region>
      {message}
    </div>
  {/if}

  {#if pluginErrors.length > 0}
    <div class="plugin-errors" data-tauri-drag-region>
      {#each pluginErrors as err}
        <span class="plugin-error-item" title={err.message}>{err.plugin_name} failed</span>
      {/each}
    </div>
  {/if}

  <div class="content" data-tauri-drag-region>
    <div class="transfer-layout" data-tauri-drag-region>
      <section class="card clipboard-section" data-tauri-drag-region>
        <header class="card-header">
          <h2 class="h4">Clipboard</h2>
          {#if clipboardFiles.length}
            <span class="badge badge-primary" data-tauri-drag-region>{clipboardFiles.length} {clipboardFiles.length === 1 ? 'file' : 'files'}</span>
          {:else if clipboardContent}
            <span class="badge badge-primary" data-tauri-drag-region>{clipboardHtml ? 'rich text · ' : ''}{clipboardContent.length} chars</span>
          {:else if clipboardImage}
            <span class="badge badge-primary" data-tauri-drag-region>image</span>
          {/if}
        </header>

        <div class="card-body" data-tauri-drag-region>
          {#if clipboardFiles.length}
            <div class="clipboard-files" data-tauri-drag-region>
              {#each clipboardFiles as f}
                <div class="file-row" title={f}>
                  <span class="file-icon">📄</span>
                  <span class="file-name">{fileName(f)}</span>
                </div>
              {/each}
            </div>
          {:else if clipboardContent}
            <div class="clipboard-content" data-tauri-drag-region>
              {clipboardContent}
            </div>
          {:else if clipboardImage}
            <div class="clipboard-image" data-tauri-drag-region>
              <img src="data:image/png;base64,{clipboardImage}" alt="Clipboard" />
            </div>
          {:else}
            <div class="empty-state compact" data-tauri-drag-region>
              <div class="empty-icon" data-tauri-drag-region>📄</div>
              <h3 class="h6" data-tauri-drag-region>No compatible content</h3>
              <p class="text-secondary" data-tauri-drag-region>Copy text, an image, or files</p>
            </div>
          {/if}
        </div>
      </section>

      <div class="transfer-arrow" data-tauri-drag-region>
        <div class="arrow-container" data-tauri-drag-region>
          {#if sendingTo}
            <div class="spinner small" data-tauri-drag-region></div>
          {:else}
            <div class="arrow" data-tauri-drag-region>→</div>
          {/if}
        </div>
      </div>

      <section class="card targets-section" data-tauri-drag-region>
        <header class="card-header" data-tauri-drag-region>
          <h2 class="h4" data-tauri-drag-region>Targets</h2>
          {#if targets.length > 0 && clipboardFormat !== null}
            <span class="badge badge-success" data-tauri-drag-region>{compatibleCount}/{targets.length} compatible</span>
          {:else if targets.length > 0}
            <span class="badge badge-success" data-tauri-drag-region>{targets.length} available</span>
          {/if}
        </header>
        
        <div class="card-body">
          {#if loadingTargets}
            <div class="loading-state compact" data-tauri-drag-region>
              <div class="spinner" data-tauri-drag-region></div>
              <span data-tauri-drag-region>Scanning...</span>
            </div>
          {:else if targets.length > 0}
            <div class="targets-grid">
              {#each targets as target, index}
                {@const compatible = isCompatible(target)}
                <button
                  class="target-card compact"
                  class:sending={sendingTo === target.id}
                  class:disabled={sendingTo !== null && sendingTo !== target.id}
                  class:unsupported={!compatible}
                  class:selected={index === selectedTargetIndex && compatible}
                  disabled={sendingTo !== null}
                  title={!compatible ? `Requires: ${target.formats.join(', ')}` : undefined}
                  on:click={() => sendToTarget(target, true)}
                  data-tauri-drag-region="false"
                >
                  <div class="target-avatar small">
                    <img
                      src="data:image/png;base64,{target.image}"
                      alt={target.title}
                      on:error={handleImageError}
                    />
                    <div class="avatar-fallback" style="display: none;">
                      {target.title.substring(0, 1).toUpperCase()}
                    </div>
                  </div>

                  <div class="target-info">
                    <h3 class="target-title">{target.title}</h3>
                    <p class="target-description text-secondary">{target.description}</p>
                    <div class="target-badges">
                      <span class="badge badge-provider">{target.provider}</span>
                      {#if !compatible}
                        <span class="badge badge-format-hint">{target.formats.join(' · ')} only</span>
                      {/if}
                    </div>
                  </div>

                  <div class="target-action">
                    {#if sendingTo === target.id}
                      <div class="spinner micro"></div>
                    {:else if !compatible}
                      <span class="incompatible-icon">⊘</span>
                    {:else}
                      <span class="send-arrow">→</span>
                    {/if}
                  </div>
                </button>
              {/each}
            </div>
          {:else}
            <div class="empty-state compact" data-tauri-drag-region>
              <div class="empty-icon" data-tauri-drag-region>🎯</div>
              <h3 class="h6" data-tauri-drag-region>No targets</h3>
              <p class="text-secondary" data-tauri-drag-region>Enable providers in settings</p>
              <button 
                class="btn btn-primary btn-xs" 
                on:click={loadTargets}
                data-tauri-drag-region="false"
              >
                Rescan
              </button>
            </div>
          {/if}
        </div>
      </section>
    </div>
  </div>

  <footer class="footer" data-tauri-drag-region>
    <div class="footer-content text-center" data-tauri-drag-region>
      <span class="footer-hint text-muted" data-tauri-drag-region><kbd class="key" data-tauri-drag-region>↑↓</kbd> select · <kbd class="key" data-tauri-drag-region>Enter</kbd> send · <kbd class="key" data-tauri-drag-region>Esc</kbd> hide</span>
    </div>
  </footer>
</div>

<style>
  .plugin-errors {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-sm) var(--space-md);
    padding: var(--space-sm) var(--space-lg);
    background: color-mix(in srgb, var(--accent-danger) 8%, transparent);
    border-bottom: 1px solid color-mix(in srgb, var(--accent-danger) 25%, transparent);
  }

  .plugin-error-item {
    font-size: 0.75rem;
    color: var(--accent-danger);
    cursor: default;
  }

  .content {
    flex: 1;
    padding: var(--space-md);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }

  .transfer-layout {
    flex: 1;
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    gap: var(--space-md);
    align-items: stretch;
  }

  .clipboard-section,
  .targets-section {
    display: flex;
    flex-direction: column;
    min-height: 240px;
  }

  .clipboard-section .card-body,
  .targets-section .card-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .card-header {
    height: 34px;
    padding: 0 var(--space-lg);
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-bottom: 1px solid var(--border-primary);
  }

  .card-header h2 {
    margin: 0;
    font-family: var(--font-primary);
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .card-body {
    padding: var(--space-md);
  }

  .header-actions {
    display: flex;
    gap: var(--space-sm);
  }

  .icon.spinning {
    display: inline-block;
    animation: spin 0.8s linear infinite;
  }

  .transfer-arrow {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .arrow-container {
    width: 26px;
    height: 26px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-surface);
    border: 1px solid var(--border-primary);
    border-radius: 50%;
  }

  .arrow {
    font-size: 0.9rem;
    color: var(--accent-primary);
  }

  .clipboard-content {
    background: var(--bg-secondary);
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-md);
    padding: var(--space-md) var(--space-lg);
    font-family: var(--font-mono);
    font-size: 0.75rem;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-word;
    flex: 1;
    overflow-y: auto;
  }

  .clipboard-files {
    background: var(--bg-secondary);
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-md);
    padding: var(--space-sm);
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .file-row {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    min-height: 24px;
    padding: 0 var(--space-sm);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: 0.75rem;
    color: var(--text-secondary);
  }

  .file-row:hover {
    background: var(--bg-hover);
  }

  .file-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .clipboard-image {
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-md);
    overflow: hidden;
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-secondary);
  }

  .clipboard-image img {
    max-width: 100%;
    max-height: 246px;
    object-fit: contain;
    display: block;
  }

  .targets-grid {
    display: grid;
    gap: 2px;
    align-content: start;
    flex: 1;
    overflow-y: auto;
  }

  .target-card {
    display: flex;
    align-items: center;
    gap: var(--space-md);
    padding: var(--space-sm) var(--space-md);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-md);
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast), border-color var(--transition-fast);
    width: 100%;
    color: inherit;
  }

  .target-card:hover:not(:disabled) {
    background: var(--bg-hover);
  }

  .target-card.selected {
    background: var(--accent-soft);
    border-color: var(--accent-ring);
  }

  .target-card.sending {
    background: var(--accent-soft);
    border-color: var(--accent-primary);
  }

  .target-card.disabled {
    opacity: 0.5;
  }

  .target-card:disabled {
    cursor: not-allowed;
  }

  .target-card.unsupported {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .target-card.unsupported:hover {
    background: transparent;
  }

  .badge-format-hint {
    background: transparent;
    color: var(--text-muted);
    border: 1px solid var(--border-primary);
  }

  .incompatible-icon {
    font-size: 0.9rem;
    color: var(--text-muted);
  }

  .target-avatar {
    position: relative;
    width: 32px;
    height: 32px;
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--bg-tertiary);
    flex-shrink: 0;
  }

  .target-avatar.small {
    width: 28px;
    height: 28px;
  }

  .target-avatar img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .avatar-fallback {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--accent-primary);
    color: var(--on-accent);
    font-weight: 600;
    font-size: 0.8rem;
  }

  .target-info {
    flex: 1;
    min-width: 0;
  }

  .target-title {
    margin: 0;
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .target-description {
    margin: 0 0 2px 0;
    font-size: 0.75rem;
    line-height: 1.3;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .target-badges {
    display: flex;
    gap: var(--space-xs);
    flex-wrap: wrap;
  }

  .target-action {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 20px;
  }

  .send-arrow {
    font-size: 0.9rem;
    color: var(--text-muted);
    transition: transform var(--transition-fast), color var(--transition-fast);
  }

  .target-card:hover .send-arrow,
  .target-card.selected .send-arrow {
    transform: translateX(2px);
    color: var(--accent-primary);
  }

  .badge {
    display: inline-flex;
    align-items: center;
    height: 16px;
    padding: 0 6px;
    border-radius: var(--radius-sm);
    font-size: 0.68rem;
    font-weight: 500;
  }

  .badge-primary {
    background: var(--accent-soft);
    color: var(--accent-primary);
  }

  .badge-success {
    background: color-mix(in srgb, var(--accent-success) 12%, transparent);
    color: var(--accent-success);
  }

  .badge-provider {
    background: var(--bg-tertiary);
    color: var(--text-secondary);
  }

  .empty-state {
    text-align: center;
    padding: var(--space-xl);
  }

  .empty-state.compact {
    padding: var(--space-lg);
  }

  .empty-icon {
    font-size: 1.5rem;
    margin-bottom: var(--space-md);
    opacity: 0.5;
  }

  .empty-state h3 {
    margin-bottom: var(--space-xs);
    color: var(--text-primary);
    font-size: 0.95rem;
  }

  .empty-state p {
    margin-bottom: var(--space-md);
    color: var(--text-muted);
    font-size: 0.8rem;
  }

  .loading-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-md);
    padding: var(--space-xl);
  }

  .loading-state.compact {
    padding: var(--space-lg);
    gap: var(--space-sm);
  }

  .loading-state span {
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .footer {
    height: 26px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-top: 1px solid var(--border-primary);
    background: var(--bg-secondary);
  }

  .footer-content {
    display: flex;
    justify-content: center;
    align-items: center;
  }

  .footer-hint {
    font-size: 0.72rem;
  }

  .key {
    display: inline-block;
    min-width: 18px;
    padding: 0 4px;
    background: var(--bg-surface);
    border: 1px solid var(--border-secondary);
    border-bottom-width: 2px;
    border-radius: var(--radius-sm);
    font-size: 0.68rem;
    font-family: var(--font-mono);
    line-height: 14px;
    text-align: center;
    color: var(--text-secondary);
  }

  .btn-xs {
    height: 22px;
    padding: 0 var(--space-md);
    font-size: 0.75rem;
  }

  .spinner.micro {
    width: 12px;
    height: 12px;
    border-width: 1.5px;
  }

  .app-title {
    font-family: var(--font-serif);
    font-size: 1.05rem;
    font-weight: 500;
    margin: 0;
  }

  .header-content {
    height: 38px;
    padding: 0 var(--space-md) 0 var(--space-lg);
  }
</style>