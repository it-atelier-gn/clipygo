<script lang="ts">
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { invoke } from '@tauri-apps/api/core';
  import { onDestroy, onMount } from 'svelte';
  import { browser } from '$app/env';
  import { writeText } from 'tauri-plugin-clipboard-api';
  import { openUrl } from '@tauri-apps/plugin-opener';
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

  const URL_RE = /https?:\/\/[^\s<>"{}|\\^`\[\]]+/i;

  function extractUrl(text: string): string | null {
    const m = text.match(URL_RE);
    return m ? m[0] : null;
  }

  async function openLink(notif: Notification) {
    const url = extractUrl(notif.content);
    if (url) await openUrl(url);
  }

  onMount(async () => {
    if (!browser) return;

    unlistenDirectMessage = await listen<IncomingMessage>('notification-message', (event) => {
      addNotification(event.payload);
    });

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
          {#if extractUrl(notif.content)}
            <button class="btn-action btn-open" on:click={() => openLink(notif)}>
              Open
            </button>
          {/if}
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
    background: var(--bg-primary);
  }

  .drag-strip {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 28px;
    padding: 0 var(--space-xs) 0 var(--space-md);
    border-bottom: 1px solid var(--border-primary);
    user-select: none;
    flex-shrink: 0;
  }

  .drag-label {
    font-family: var(--font-serif);
    font-size: 0.85rem;
    color: var(--text-secondary);
  }

  .drag-strip-actions {
    display: flex;
    align-items: center;
    gap: var(--space-xs);
  }

  .btn-strip {
    height: 22px;
    min-width: 22px;
    padding: 0 var(--space-sm);
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    font-size: 0.72rem;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }

  .btn-strip:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
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
    background: var(--bg-surface);
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-lg);
    overflow: hidden;
    box-shadow: var(--shadow-md);
  }

  .notification-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--space-md) var(--space-md) 0;
  }

  .sender { display: flex; align-items: center; gap: var(--space-md); min-width: 0; }

  .sender-avatar {
    width: 20px;
    height: 20px;
    flex-shrink: 0;
    border-radius: 50%;
    background: var(--accent-primary);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.68rem;
    font-weight: 600;
    color: var(--on-accent);
  }

  .sender-name {
    font-size: 0.82rem;
    font-weight: 600;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .timestamp {
    font-size: 0.7rem;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .notification-body {
    padding: var(--space-sm) var(--space-md);
  }

  .content-preview {
    margin: 0 0 var(--space-sm);
    font-family: var(--font-mono);
    font-size: 0.75rem;
    line-height: 1.45;
    color: var(--text-primary);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .content-preview:last-child { margin-bottom: 0; }

  .notification-actions {
    display: flex;
    gap: var(--space-sm);
    padding: 0 var(--space-md) var(--space-md);
  }

  .btn-action {
    height: 22px;
    padding: 0 var(--space-md);
    border-radius: var(--radius-md);
    border: 1px solid var(--border-secondary);
    background: var(--bg-surface);
    color: var(--text-primary);
    font-size: 0.75rem;
    cursor: pointer;
    transition: background var(--transition-fast);
  }

  .btn-action:hover { background: var(--bg-hover); }

  .btn-copy {
    background: var(--accent-primary);
    border-color: var(--accent-primary);
    color: var(--on-accent);
  }

  .btn-copy:hover { background: var(--accent-primary-dark); }

  .btn-dismiss {
    border-color: transparent;
    background: transparent;
    color: var(--text-muted);
    margin-left: auto;
  }

  .empty-state {
    text-align: center;
    padding: var(--space-lg);
    color: var(--text-muted);
    font-size: 0.8rem;
  }
</style>
