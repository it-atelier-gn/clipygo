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
    top: 8px;
    right: 8px;
    width: 24px;
    height: 24px;
    padding: 0;
    background: transparent;
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    font-size: 11px;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all var(--transition-fast);
    z-index: 10;
  }

  .close-btn:hover {
    background: var(--accent-danger);
    border-color: var(--accent-danger);
    color: white;
  }

  .about-body {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-sm);
    padding: var(--space-xl) 0;
  }

  .about-logo {
    width: 80px;
    height: 80px;
    margin-bottom: var(--space-sm);
    border-radius: var(--radius-lg);
    filter: drop-shadow(0 0 12px rgba(0, 212, 255, 0.4));
  }

  .about-title {
    font-family: var(--font-gaming);
    font-size: 1.75rem;
    font-weight: 700;
    background: linear-gradient(135deg, var(--accent-primary), var(--accent-secondary));
    -webkit-background-clip: text;
    -webkit-text-fill-color: transparent;
    background-clip: text;
    margin: 0;
  }

  .about-version {
    font-family: var(--font-mono);
    font-size: 0.9rem;
    color: var(--accent-primary);
    letter-spacing: 0.05em;
  }

  .about-description {
    color: var(--text-secondary);
    font-size: 0.85rem;
    text-align: center;
    max-width: 260px;
    line-height: 1.5;
  }

  .about-link {
    background: none;
    border: none;
    color: var(--text-muted);
    font-size: 0.8rem;
    cursor: pointer;
    padding: 0;
    transition: color var(--transition-fast);
  }

  .about-link:hover {
    color: var(--accent-primary);
  }

  .about-copyright {
    color: var(--text-muted);
    font-size: 0.75rem;
    margin-top: var(--space-md);
  }
</style>
