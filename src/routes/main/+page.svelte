<script lang="ts">
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { listen } from '@tauri-apps/api/event';
  import { onDestroy, onMount } from "svelte";
  import { invoke } from '@tauri-apps/api/core';
  import { browser } from '$app/environment';
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { hasText, readText, hasImage, readImageBase64 } from "tauri-plugin-clipboard-api";

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

  let unlistenWindowEvents: UnlistenFn;
  let unlistenClipboard: UnlistenFn;
  let clipboardContent = "";
  let clipboardImage = ""; // base64 PNG when clipboard has an image
  let targets: Target[] = [];
  let pluginErrors: PluginError[] = [];
  let loadingTargets = false;
  let sendingTo: string | null = null;
  let message = "";
  let messageType: 'success' | 'error' | '' = '';
  let selectedTargetIndex = 0;

  $: clipboardFormat = clipboardContent ? 'text' : clipboardImage ? 'image' : null;
  $: compatibleCount = targets.filter(t => clipboardFormat !== null && t.formats.includes(clipboardFormat)).length;

  function isCompatible(target: Target): boolean {
    return clipboardFormat !== null && target.formats.includes(clipboardFormat);
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

  // Handle image error - show fallback instead
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
      if (await hasText()) {
        clipboardContent = await readText();
        clipboardImage = "";
      } else if (await hasImage()) {
        clipboardImage = await readImageBase64();
        clipboardContent = "";
      } else {
        clipboardContent = "";
        clipboardImage = "";
      }
    } catch (e) {
      console.error('Failed to read clipboard:', e);
    }
  }

  async function loadTargets() {
    loadingTargets = true;
    try {
      const result: GetTargetsResult = await invoke('get_targets');
      targets = result.targets;
      pluginErrors = result.errors;
      selectedTargetIndex = 0; // Reset selection
    } catch (error) {
      console.error('Failed to load targets:', error);
      showMessage('Failed to load targets', 'error');
    } finally {
      loadingTargets = false;
    }
  }

  async function sendToTarget(target: Target, fromClick = false) {
    if (!clipboardFormat) {
      showMessage('No clipboard content to send', 'error');
      return;
    }

    if (!isCompatible(target)) {
      showMessage(`${target.title} does not support ${clipboardFormat} content`, 'error');
      return;
    }

    // Update selected index if clicked
    if (fromClick) {
      selectedTargetIndex = targets.findIndex(t => t.id === target.id);
    }

    sendingTo = target.id;
    try {
      const payload: SendPayload = {
        content: clipboardFormat === 'image' ? clipboardImage : clipboardContent,
        format: clipboardFormat
      };
      
      await invoke('send_to_target', { 
        targetId: target.id, 
        payload 
      });

      showMessage(`Content sent to ${target.title}`, 'success');

      // Hide window after successful send
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

    // Refresh on window focus (e.g. hotkey shows window)
    unlistenWindowEvents = await listen('tauri://focus', async () => {
      await readClipboard();
      await loadTargets();
    });

    // Also refresh whenever clipboard changes while window is open
    unlistenClipboard = await listen('plugin:clipboard://clipboard-monitor/update', async () => {
      await readClipboard();
    });

    // Load initial content and targets
    await readClipboard();
    await loadTargets();
  });

  onDestroy(() => {
    if (!browser) return;
    document.removeEventListener('keydown', handleKeydown);
    if (unlistenWindowEvents) unlistenWindowEvents();
    if (unlistenClipboard) unlistenClipboard();
  });

  // Reactive statement to ensure selectedTargetIndex stays in bounds
  $: if (targets.length > 0 && selectedTargetIndex >= targets.length) {
    selectedTargetIndex = 0;
  }
</script>

<div class="app" data-tauri-drag-region>
  <!-- Header -->
  <header class="header" data-tauri-drag-region>
    <div class="header-content flex justify-between items-center" data-tauri-drag-region>
      <h1 class="h2 title-shimmer" data-tauri-drag-region>📋 clipygo</h1>
      <div class="header-actions flex gap-sm">
        <button
          class="btn btn-secondary btn-sm"
          on:click={async () => { await readClipboard(); await loadTargets(); }}
          disabled={loadingTargets}
        >
          <span class="icon" class:spinning={loadingTargets}>↻</span>
          REFRESH
        </button>
        <button
          class="btn btn-danger btn-sm close-btn-aligned"
          on:click={() => getCurrentWebviewWindow().hide()}
        >
          ✕
        </button>
      </div>
    </div>
  </header>

  <!-- Message -->
  {#if message}
    <div class="message message-{messageType}" data-tauri-drag-region>
      {message}
    </div>
  {/if}

  <!-- Plugin error warnings -->
  {#if pluginErrors.length > 0}
    <div class="plugin-errors" data-tauri-drag-region>
      {#each pluginErrors as err}
        <span class="plugin-error-item" title={err.message}>{err.plugin_name} failed</span>
      {/each}
    </div>
  {/if}

  <div class="content" data-tauri-drag-region>
    <div class="transfer-layout" data-tauri-drag-region>
      <!-- Clipboard Content (Left) -->
      <section class="card clipboard-section" data-tauri-drag-region>
        <header class="card-header">
          <h2 class="h4 clipboard-header">📄 Clipboard Content</h2>
          {#if clipboardContent}
            <span class="badge badge-primary" data-tauri-drag-region>{clipboardContent.length} chars</span>
          {:else if clipboardImage}
            <span class="badge badge-primary" data-tauri-drag-region>image</span>
          {/if}
        </header>

        <div class="card-body" data-tauri-drag-region>
          {#if clipboardContent}
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
              <p class="text-secondary" data-tauri-drag-region>Copy text or an image</p>
            </div>
          {/if}
        </div>
      </section>

      <!-- Transfer Arrow (Middle) -->
      <div class="transfer-arrow" data-tauri-drag-region>
        <div class="arrow-container" data-tauri-drag-region>
          {#if sendingTo}
            <div class="spinner small" data-tauri-drag-region></div>
          {:else}
            <div class="arrow" data-tauri-drag-region>→</div>
          {/if}
        </div>
      </div>

      <!-- Targets (Right) -->
      <section class="card targets-section" data-tauri-drag-region>
        <header class="card-header" data-tauri-drag-region>
          <h2 class="h4" data-tauri-drag-region>🎯 Targets</h2>
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
                        <span class="badge badge-format-hint">{target.formats.join(' · ').toUpperCase()} ONLY</span>
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
              <h3 class="h6" data-tauri-drag-region>No Targets</h3>
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

  <!-- Footer -->
  <footer class="footer" data-tauri-drag-region>
    <div class="footer-content text-center" data-tauri-drag-region>
      <span class="footer-hint text-muted" data-tauri-drag-region>Use <kbd class="key" data-tauri-drag-region>↑↓</kbd> to select • <kbd class="key" data-tauri-drag-region>ENTER</kbd> to send • <kbd class="key" data-tauri-drag-region>ESC</kbd> to hide</span>
    </div>
  </footer>
</div>

<style>
  .plugin-errors {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-xs);
    padding: var(--space-xs) var(--space-md);
    background: rgba(255, 80, 80, 0.1);
    border-bottom: 1px solid rgba(255, 80, 80, 0.3);
  }

  .plugin-error-item {
    font-size: 0.7rem;
    font-family: var(--font-gaming);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: #ff5050;
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
    min-height: 300px;
  }

  .clipboard-section .card-body,
  .targets-section .card-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .card-header {
    padding: var(--space-sm) var(--space-md);
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-bottom: 1px solid var(--border-primary);
  }

  .card-header h2 {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
  }

  .card-body {
    padding: var(--space-md);
  }

  /* Header Actions */
  .header-actions {
    display: flex;
    gap: var(--space-sm);
  }

  .icon.spinning {
    animation: spin 1s linear infinite;
  }

  /* Transfer Arrow */
  .transfer-arrow {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .arrow-container {
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, var(--bg-elevated), var(--bg-surface));
    border: 2px solid var(--border-primary);
    border-radius: 50%;
    box-shadow: var(--shadow-md);
  }

  .arrow {
    font-size: 1.5rem;
    color: var(--accent-primary);
    font-family: var(--font-gaming);
    text-shadow: 0 0 10px var(--accent-primary);
  }

  /* Clipboard Content */
  .clipboard-content {
    background: linear-gradient(135deg, var(--bg-tertiary), var(--bg-secondary));
    border: 2px solid var(--border-accent);
    border-radius: var(--radius-md);
    padding: var(--space-md);
    font-family: var(--font-mono);
    font-size: 0.8rem;
    line-height: 1.4;
    white-space: pre-wrap;
    word-break: break-word;
    flex: 1;
    overflow-y: auto;
    position: relative;
    box-shadow: inset 0 2px 8px rgba(0, 0, 0, 0.3);
  }

  .clipboard-image {
    border: 2px solid var(--border-accent);
    border-radius: var(--radius-md);
    overflow: hidden;
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-tertiary);
  }

  .clipboard-image img {
    max-width: 100%;
    max-height: 246px;
    object-fit: contain;
    display: block;
  }

  .clipboard-content::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    width: 3px;
    height: 100%;
    background: linear-gradient(135deg, var(--accent-primary), var(--accent-secondary));
    border-radius: 2px 0 0 2px;
  }

  /* Targets */
  .targets-grid {
    display: grid;
    gap: var(--space-sm);
    align-content: start;
    flex: 1;
    overflow-y: auto;
  }

  .target-card {
    display: flex;
    align-items: center;
    gap: var(--space-md);
    padding: var(--space-md);
    background: linear-gradient(135deg, var(--bg-elevated), var(--bg-surface));
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-md);
    cursor: pointer;
    text-align: left;
    transition: all var(--transition-normal);
    width: 100%;
    position: relative;
    overflow: hidden;
  }

  .target-card.compact {
    padding: var(--space-sm) var(--space-md);
  }

  .target-card::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    width: 3px;
    height: 100%;
    background: linear-gradient(135deg, var(--accent-primary), var(--accent-secondary));
    opacity: 0;
    transition: opacity var(--transition-normal);
  }

  .target-card:hover:not(:disabled) {
    border-color: var(--accent-primary);
    box-shadow: var(--shadow-lg), var(--glow-primary);
    transform: translateY(-1px);
  }

  .target-card:hover:not(:disabled)::before {
    opacity: 1;
  }

  .target-card.selected {
    border-color: var(--accent-primary);
    box-shadow: var(--shadow-md), var(--glow-primary);
    background: linear-gradient(135deg, rgba(0, 212, 255, 0.1), var(--bg-surface));
  }

  .target-card.selected::before {
    opacity: 1;
  }

  .target-card.sending {
    border-color: var(--accent-primary);
    background: linear-gradient(135deg, rgba(0, 212, 255, 0.15), var(--bg-surface));
    box-shadow: var(--glow-primary);
  }

  .target-card.sending::before {
    opacity: 1;
    animation: pulse 2s infinite;
  }

  .target-card.disabled {
    opacity: 0.5;
  }

  .target-card:disabled {
    cursor: not-allowed;
  }

  .target-card.unsupported {
    filter: grayscale(0.8);
    opacity: 0.45;
    cursor: not-allowed;
  }

  .target-card.unsupported:hover {
    border-color: var(--border-primary);
    box-shadow: none;
    transform: none;
  }

  .target-card.unsupported:hover::before {
    opacity: 0;
  }

  .badge-format-hint {
    background: rgba(255, 255, 255, 0.05);
    color: var(--text-muted);
    border: 1px solid var(--border-secondary);
  }

  .incompatible-icon {
    font-size: 1.1rem;
    color: var(--text-muted);
    opacity: 0.6;
  }

  .target-avatar {
    position: relative;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-primary);
    box-shadow: var(--shadow-sm);
  }

  .target-avatar.small {
    width: 32px;
    height: 32px;
  }

  .target-avatar img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .avatar-fallback {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: linear-gradient(135deg, var(--accent-primary), var(--accent-purple));
    color: var(--text-primary);
    font-weight: 700;
    font-size: 0.9rem;
    font-family: var(--font-gaming);
  }

  .target-info {
    flex: 1;
    min-width: 0;
  }

  .target-title {
    margin: 0 0 2px 0;
    font-family: var(--font-gaming);
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--text-primary);
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .target-description {
    margin: 0 0 var(--space-xs) 0;
    font-size: 0.75rem;
    line-height: 1.3;
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
    width: 30px;
    height: 30px;
  }

  .send-arrow {
    font-size: 1.2rem;
    color: var(--text-muted);
    transition: all var(--transition-normal);
    font-family: var(--font-gaming);
  }

  .target-card:hover .send-arrow,
  .target-card.selected .send-arrow {
    transform: translateX(2px);
    color: var(--accent-primary);
    text-shadow: 0 0 8px var(--accent-primary);
  }

  /* Badges */
  .badge {
    display: inline-flex;
    align-items: center;
    padding: 2px var(--space-sm);
    border-radius: var(--radius-sm);
    font-size: 0.65rem;
    font-weight: 600;
    font-family: var(--font-gaming);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .badge-primary {
    background: linear-gradient(135deg, rgba(0, 212, 255, 0.2), rgba(0, 212, 255, 0.1));
    color: var(--accent-primary);
    border: 1px solid var(--accent-primary);
  }

  .badge-success {
    background: linear-gradient(135deg, rgba(0, 255, 136, 0.2), rgba(0, 255, 136, 0.1));
    color: var(--accent-success);
    border: 1px solid var(--accent-success);
  }

  .badge-provider {
    background: linear-gradient(135deg, rgba(255, 107, 53, 0.2), rgba(255, 107, 53, 0.1));
    color: var(--accent-secondary);
    border: 1px solid var(--accent-secondary);
  }

  /* States */
  .empty-state {
    text-align: center;
    padding: var(--space-xl);
  }

  .empty-state.compact {
    padding: var(--space-lg);
  }

  .empty-icon {
    font-size: 2.5rem;
    margin-bottom: var(--space-md);
    opacity: 0.6;
  }

  .empty-state h3 {
    margin-bottom: var(--space-sm);
    font-family: var(--font-gaming);
    color: var(--text-primary);
    font-size: 0.9rem;
  }

  .empty-state p {
    margin-bottom: var(--space-sm);
    color: var(--text-secondary);
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
    color: var(--text-secondary);
  }

  /* Footer */
  .footer {
    padding: var(--space-sm) var(--space-md);
    border-top: 1px solid var(--border-primary);
    background: linear-gradient(135deg, var(--bg-secondary), var(--bg-tertiary));
  }

  .footer-content {
    display: flex;
    justify-content: center;
    align-items: center;
  }

  .footer-hint {
    font-size: 0.7rem;
    font-family: var(--font-gaming);
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }

  .key {
    background: linear-gradient(135deg, var(--bg-elevated), var(--bg-surface));
    border: 1px solid var(--border-secondary);
    border-radius: var(--radius-sm);
    padding: 2px var(--space-xs);
    font-size: 0.7rem;
    font-family: var(--font-mono);
    font-weight: 600;
    color: var(--accent-primary);
    text-shadow: 0 0 5px var(--accent-primary);
    box-shadow: var(--shadow-sm);
  }

  /* Button sizes */
  .btn-xs {
    padding: var(--space-xs) var(--space-sm);
    font-size: 0.7rem;
  }

  /* Spinner sizes */
  .spinner {
    width: 24px;
    height: 24px;
    border: 2px solid var(--border-primary);
    border-top: 2px solid var(--accent-primary);
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }

  .spinner.small {
    width: 20px;
    height: 20px;
  }

  .spinner.micro {
    width: 16px;
    height: 16px;
    border-width: 1px;
  }

  /* Title shimmer */
  .title-shimmer {
    background: linear-gradient(
      90deg,
      var(--accent-primary) 20%,
      #ffffff 50%,
      var(--accent-primary) 80%
    );
    background-size: 200% auto;
    background-clip: text;
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    animation: shimmer 3s linear infinite;
  }

  @keyframes shimmer {
    0%   { background-position: 200% center; }
    100% { background-position: -200% center; }
  }

  /* Animations */
  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.5; }
  }

  @keyframes spin {
    0% { transform: rotate(0deg); }
    100% { transform: rotate(360deg); }
  }

  /* Header height reduction */
  .header-content {
    padding: var(--space-md) 0;
  }

  /* Align close button with target list */
  .close-btn-aligned {
    margin-right: var(--space-md);
  }

  /* Align clipboard icon with content box */
  .clipboard-header {
    margin-left: calc(3px + var(--space-md));
  }
</style>