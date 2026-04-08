<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { invoke } from '@tauri-apps/api/core';
  import { onDestroy, onMount } from 'svelte';
  import { browser } from '$app/environment';
  import { writeText } from 'tauri-plugin-clipboard-api';
  import type { UnlistenFn } from '@tauri-apps/api/event';

  interface IncomingMessage {
    from_name: string;
    from_id: string;
    content: string;
    format: string;
    timestamp: number;
  }

  interface Notification {
    id: number;
    from_name: string;
    content: string;
    format: string;
    timestamp: number;
    dismissTimer?: ReturnType<typeof setTimeout>;
  }

  const AUTO_DISMISS_MS = 30_000;

  let notifications: Notification[] = [];
  let nextId = 0;
  let unlistenDirectMessage: UnlistenFn;
  let copiedId: number | null = null;

  function addNotification(msg: IncomingMessage) {
    const id = nextId++;
    const notif: Notification = {
      id,
      from_name: msg.from_name,
      content: msg.content,
      format: msg.format,
      timestamp: msg.timestamp,
    };

    notif.dismissTimer = setTimeout(() => dismiss(id), AUTO_DISMISS_MS);
    notifications = [...notifications, notif];
  }

  function dismiss(id: number) {
    const notif = notifications.find(n => n.id === id);
    if (notif?.dismissTimer) clearTimeout(notif.dismissTimer);
    notifications = notifications.filter(n => n.id !== id);

    if (notifications.length === 0) {
      getCurrentWebviewWindow().hide();
    }
  }

  function dismissAll() {
    for (const n of notifications) {
      if (n.dismissTimer) clearTimeout(n.dismissTimer);
    }
    notifications = [];
    getCurrentWebviewWindow().hide();
  }

  async function copyContent(notif: Notification) {
    try {
      await writeText(notif.content);
      copiedId = notif.id;
      setTimeout(() => { copiedId = null; }, 1500);
    } catch (e) {
      console.error('Failed to copy:', e);
    }
  }

  function formatTime(ts: number): string {
    return new Date(ts * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  }

  function truncate(text: string, max: number): string {
    return text.length > max ? text.slice(0, max) + '...' : text;
  }

  onMount(async () => {
    if (!browser) return;

    // Direct delivery: Rust emits this globally when the window already exists.
    // Uses global listen (not window-scoped) so it receives broadcasts from app.emit().
    unlistenDirectMessage = await listen<IncomingMessage>('notification-message', (event) => {
      addNotification(event.payload);
    });

    // Drain the queue for messages that arrived before this window's JS was ready (first load only).
    const queued = await invoke<IncomingMessage[]>('get_pending_notifications');
    for (const msg of queued) {
      addNotification(msg);
    }

    if (notifications.length === 0) {
      getCurrentWebviewWindow().hide();
    }
  });

  onDestroy(() => {
    if (!browser) return;
    if (unlistenDirectMessage) unlistenDirectMessage();
    for (const n of notifications) {
      if (n.dismissTimer) clearTimeout(n.dismissTimer);
    }
  });
</script>

<div class="notification-container">
  <div class="drag-strip" data-tauri-drag-region>
    <span class="drag-label" data-tauri-drag-region>clipygo</span>
    <div class="drag-strip-actions" data-tauri-drag-region>
      {#if notifications.length > 1}
        <button class="btn-strip" data-tauri-drag-region="false" on:click={dismissAll}>
          Dismiss all ({notifications.length})
        </button>
      {/if}
      <button class="btn-strip btn-strip-close" data-tauri-drag-region="false" on:click={() => getCurrentWebviewWindow().hide()}>✕</button>
    </div>
  </div>

  <div class="notifications-list">
    {#each notifications as notif (notif.id)}
      <div class="notification-card">
        <div class="notification-header">
          <div class="sender">
            <span class="sender-avatar">{notif.from_name.charAt(0).toUpperCase()}</span>
            <span class="sender-name">{notif.from_name}</span>
          </div>
          <span class="timestamp">{formatTime(notif.timestamp)}</span>
        </div>

        <div class="notification-body">
          <p class="content-preview">{truncate(notif.content, 200)}</p>
        </div>

        <div class="notification-actions">
          <button class="btn-action btn-copy" on:click={() => copyContent(notif)}>
            {copiedId === notif.id ? 'Copied!' : 'Copy'}
          </button>
          <button class="btn-action btn-dismiss" on:click={() => dismiss(notif.id)}>
            Dismiss
          </button>
        </div>
      </div>
    {/each}

    {#if notifications.length === 0}
      <div class="empty-state">No notifications</div>
    {/if}
  </div>
</div>

<style>
  .notification-container {
    display: flex;
    flex-direction: column;
    max-height: 100vh;
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

  .btn-strip-close:hover {
    color: var(--accent-secondary);
  }

  .notifications-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
    padding: var(--space-sm);
    overflow-y: auto;
    max-height: calc(100vh - 28px);
  }

  .notification-card {
    background: linear-gradient(135deg, var(--bg-elevated), var(--bg-surface));
    border: 1px solid var(--border-accent);
    border-radius: var(--radius-md);
    overflow: hidden;
    position: relative;
    box-shadow: var(--shadow-lg);
  }

  .notification-card::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    width: 3px;
    height: 100%;
    background: linear-gradient(135deg, var(--accent-primary), var(--accent-secondary));
  }

  .notification-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--space-sm) var(--space-md);
    padding-left: calc(3px + var(--space-md));
    border-bottom: 1px solid var(--border-primary);
  }

  .sender {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
  }

  .sender-avatar {
    width: 24px;
    height: 24px;
    border-radius: var(--radius-sm);
    background: linear-gradient(135deg, var(--accent-primary), var(--accent-purple));
    display: flex;
    align-items: center;
    justify-content: center;
    font-family: var(--font-gaming);
    font-size: 0.7rem;
    font-weight: 700;
    color: var(--text-primary);
  }

  .sender-name {
    font-family: var(--font-gaming);
    font-size: 0.8rem;
    font-weight: 600;
    color: var(--text-primary);
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .timestamp {
    font-family: var(--font-mono);
    font-size: 0.65rem;
    color: var(--text-muted);
  }

  .notification-body {
    padding: var(--space-sm) var(--space-md);
    padding-left: calc(3px + var(--space-md));
  }

  .content-preview {
    margin: 0;
    font-family: var(--font-mono);
    font-size: 0.75rem;
    line-height: 1.4;
    color: var(--text-secondary);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .notification-actions {
    display: flex;
    gap: var(--space-xs);
    padding: var(--space-xs) var(--space-md);
    padding-left: calc(3px + var(--space-md));
    border-top: 1px solid var(--border-primary);
  }

  .btn-action {
    font-family: var(--font-gaming);
    font-size: 0.65rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: var(--space-xs) var(--space-sm);
    border-radius: var(--radius-sm);
    border: 1px solid var(--border-primary);
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    cursor: pointer;
    transition: all var(--transition-normal);
  }

  .btn-copy:hover {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
    box-shadow: var(--glow-primary);
  }

  .btn-dismiss:hover {
    border-color: var(--accent-secondary);
    color: var(--accent-secondary);
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
