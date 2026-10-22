<script lang="ts">
  import { onMount } from 'svelte';
  import { getVersion } from '@tauri-apps/api/app';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
  import { openUrl } from '@tauri-apps/plugin-opener';

  let version = '';

  async function closeWindow() {
    await getCurrentWebviewWindow().hide();
  }

  function openGitHub() {
    openUrl('https://github.com/it-atelier-gn/clipygo');
  }

  onMount(async () => {
    version = await getVersion();
  });
</script>

<svelte:window on:keydown={(e) => e.key === 'Escape' && closeWindow()} />

<div class="app" data-tauri-drag-region>
  <button class="close-btn" on:click={closeWindow}>✕</button>
  <div class="container" data-tauri-drag-region>
    <div class="about-body" data-tauri-drag-region>
      <img src="/icon.png" alt="clipygo" class="about-logo" data-tauri-drag-region />
      <h1 class="about-title" data-tauri-drag-region>clipygo</h1>
      <p class="about-version" data-tauri-drag-region>v{version}</p>
      <p class="about-description" data-tauri-drag-region>Clipboard monitor that routes content to configured targets.</p>
      <button class="about-link" on:click={openGitHub}>github.com/it-atelier-gn/clipygo</button>
      <p class="about-copyright" data-tauri-drag-region>&copy; 2026 the clipygo contributors</p>
    </div>
  </div>
</div>

<style>
  .close-btn {
    position: fixed;
    top: 6px;
    right: 6px;
    width: 24px;
    height: 24px;
    padding: 0;
    background: transparent;
    border: none;
    border-radius: var(--radius-md);
    color: var(--text-muted);
    font-size: 11px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: background var(--transition-fast), color var(--transition-fast);
    z-index: 10;
  }

  .close-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .about-body {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-sm);
    padding: var(--space-2xl) 0 var(--space-xl);
  }

  .about-logo {
    width: 56px;
    height: 56px;
    margin-bottom: var(--space-md);
    border-radius: var(--radius-lg);
  }

  .about-title {
    font-family: var(--font-serif);
    font-size: 1.6rem;
    font-weight: 500;
    color: var(--text-primary);
    margin: 0;
  }

  .about-version {
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .about-description {
    color: var(--text-secondary);
    font-size: 0.85rem;
    text-align: center;
    max-width: 260px;
    line-height: 1.5;
    margin-top: var(--space-sm);
  }

  .about-link {
    background: none;
    border: none;
    color: var(--accent-primary);
    font-size: 0.8rem;
    cursor: pointer;
    padding: 0;
    text-decoration: underline;
    text-underline-offset: 2px;
    text-decoration-color: var(--accent-ring);
  }

  .about-link:hover {
    text-decoration-color: currentColor;
  }

  .about-copyright {
    color: var(--text-muted);
    font-size: 0.72rem;
    margin-top: var(--space-lg);
  }
</style>
