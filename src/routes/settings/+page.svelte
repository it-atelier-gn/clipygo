<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';

  function closeWindow() {
    getCurrentWebviewWindow().hide();
  }

  interface PluginProvider {
    id: string;
    name: string;
    command: string;
    enabled: boolean;
  }

  interface AppSettings {
    autostart: boolean;
    global_shortcut: string;
    regex_list: string[];
    target_providers: {
      msteams: { enabled: boolean };
      plugins: PluginProvider[];
    };
    registry_url: string;
  }

  interface RegistryPlatform {
    url: string;
    sha256: string;
  }

  interface RegistryPlugin {
    id: string;
    name: string;
    description: string;
    author: string;
    version: string;
    repo: string;
    platforms: Record<string, RegistryPlatform>;
  }

  interface Registry {
    version: number;
    plugins: RegistryPlugin[];
  }

  let settings: AppSettings | null = null;
  let loading = true;
  let saving = false;
  let message = "";
  let messageType: 'success' | 'error' | '' = '';
  let newPlugin = { name: '', command: '' };
  let newPluginTestResult: boolean | null = null;
  let pluginPathValid: Record<string, boolean> = {};

  // registry state
  let registry: Registry | null = null;
  let registryLoading = false;
  let registryError = '';
  let installingId: string | null = null;
  let installErrors: Record<string, string> = {};

  function detectPlatform(): string {
    const ua = navigator.userAgent.toLowerCase();
    if (ua.includes('win')) return 'windows-x64';
    if (ua.includes('mac')) return 'macos-arm64';
    return 'linux-x64';
  }

  async function loadRegistry() {
    registryLoading = true;
    registryError = '';
    try {
      registry = await invoke('fetch_registry');
    } catch (e) {
      registryError = `${e}`;
    } finally {
      registryLoading = false;
    }
  }

  async function installPlugin(plugin: RegistryPlugin) {
    const platform = detectPlatform();
    if (!plugin.platforms[platform]) {
      installErrors = { ...installErrors, [plugin.id]: `No binary available for platform '${platform}'` };
      return;
    }
    installingId = plugin.id;
    installErrors = { ...installErrors, [plugin.id]: '' };
    try {
      await invoke('install_registry_plugin', { plugin, platformKey: platform });
      await loadSettings();
      showMessage(`${plugin.name} installed`, 'success');
    } catch (e) {
      installErrors = { ...installErrors, [plugin.id]: `${e}` };
    } finally {
      installingId = null;
    }
  }

  function isInstalled(plugin: RegistryPlugin): boolean {
    return settings?.target_providers.plugins.some(p => p.name === plugin.name) ?? false;
  }

  // inline editing state
  let editingId: string | null = null;
  let editDraft = { name: '', command: '' };
  let editTestResult: boolean | null = null;

  function showMessage(text: string, type: 'success' | 'error') {
    message = text;
    messageType = type;
    setTimeout(() => {
      message = "";
      messageType = '';
    }, 3000);
  }

  onMount(async () => {
    await loadSettings();
    loadRegistry();
  });

  async function loadSettings() {
    try {
      settings = await invoke('get_settings');
      await checkPluginPaths();
      loading = false;
    } catch (error) {
      console.error('Failed to load settings:', error);
      loading = false;
      showMessage('Failed to load settings', 'error');
    }
  }

  async function checkPluginPaths() {
    if (!settings) return;
    const results = await Promise.all(
      settings.target_providers.plugins.map(async (p) => {
        const valid: boolean = await invoke('check_plugin_path', { command: p.command });
        return [p.id, valid] as [string, boolean];
      })
    );
    pluginPathValid = Object.fromEntries(results);
  }

  async function saveSettings() {
    if (!settings) return;
    saving = true;
    try {
      await invoke('save_settings', { settings });
      showMessage('Settings saved successfully', 'success');
    } catch (error) {
      console.error('Failed to save settings:', error);
      showMessage('Failed to save settings', 'error');
    } finally {
      saving = false;
    }
  }

  async function resetSettings() {
    try {
      settings = await invoke('reset_settings');
      showMessage('Settings reset to defaults', 'success');
    } catch (error) {
      console.error('Failed to reset settings:', error);
      showMessage('Failed to reset settings', 'error');
    }
  }

  function addRegexPattern() {
    if (settings) {
      settings.regex_list = [...settings.regex_list, ''];
    }
  }

  function removeRegexPattern(index: number) {
    if (settings) {
      settings.regex_list = settings.regex_list.filter((_, i) => i !== index);
    }
  }

  async function testNewPlugin() {
    if (!newPlugin.command.trim()) return;
    newPluginTestResult = await invoke('check_plugin_path', { command: newPlugin.command.trim() });
  }

  async function addPlugin() {
    if (!newPlugin.name.trim() || !newPlugin.command.trim()) {
      showMessage('Name and command are required', 'error');
      return;
    }
    try {
      await invoke('add_plugin', { name: newPlugin.name.trim(), command: newPlugin.command.trim() });
      await loadSettings();
      newPlugin = { name: '', command: '' };
      newPluginTestResult = null;
      showMessage('Plugin added successfully', 'success');
    } catch (error) {
      showMessage(`Failed to add plugin: ${error}`, 'error');
    }
  }

  function startEdit(plugin: PluginProvider) {
    editingId = plugin.id;
    editDraft = { name: plugin.name, command: plugin.command };
    editTestResult = pluginPathValid[plugin.id] ?? null;
  }

  function cancelEdit() {
    editingId = null;
    editTestResult = null;
  }

  async function testEditPlugin() {
    if (!editDraft.command.trim()) return;
    editTestResult = await invoke('check_plugin_path', { command: editDraft.command.trim() });
  }

  async function saveEdit(pluginId: string) {
    if (!editDraft.name.trim() || !editDraft.command.trim()) {
      showMessage('Name and command are required', 'error');
      return;
    }
    try {
      await invoke('update_plugin', { pluginId, name: editDraft.name.trim(), command: editDraft.command.trim() });
      editingId = null;
      editTestResult = null;
      await loadSettings();
    } catch (error) {
      showMessage(`Failed to update plugin: ${error}`, 'error');
    }
  }

  async function removePlugin(pluginId: string) {
    try {
      await invoke('remove_plugin', { pluginId });
      if (editingId === pluginId) editingId = null;
      await loadSettings();
      showMessage('Plugin removed', 'success');
    } catch (error) {
      showMessage(`Failed to remove plugin: ${error}`, 'error');
    }
  }

  async function togglePlugin(pluginId: string, enabled: boolean) {
    try {
      await invoke('toggle_plugin', { pluginId, enabled });
      await loadSettings();
    } catch (error) {
      showMessage(`Failed to toggle plugin: ${error}`, 'error');
    }
  }
</script>

<svelte:window on:keydown={(e) => e.key === 'Escape' && closeWindow()} />

<div class="app">
  <div class="container">
    <!-- Header -->
    <div class="header compact">
      <div class="header-content flex justify-between items-center">
        <div class="header-main" data-tauri-drag-region>
          <h1 class="h2 text-glow" data-tauri-drag-region>⚙️ clipygo Settings</h1>
          <p class="subtitle" data-tauri-drag-region>Configure your application</p>
        </div>
        <button class="btn btn-danger btn-sm close-btn-aligned" on:click={closeWindow}>
          ✕
        </button>
      </div>
    </div>

    {#if message}
      <div class="message message-{messageType}">
        {message}
      </div>
    {/if}

    {#if loading}
      <div class="card">
        <div class="loading-state">
          <div class="spinner"></div>
          <span>Initializing Configuration...</span>
        </div>
      </div>
    {:else if settings}
      <form on:submit|preventDefault={saveSettings} class="settings-form">

        <!-- General Settings -->
        <div class="card">
          <div class="card-header">
            <h2 class="h3">🔧 System Preferences</h2>
            <p class="text-secondary">Core application configuration</p>
          </div>
          <div class="card-body">
            <div class="setting-group">
              <div class="toggle-setting">
                <div class="setting-info">
                  <h3>Auto-Launch Protocol</h3>
                  <p class="text-secondary">Activate on system boot sequence</p>
                </div>
                <div class="toggle-wrapper">
                  <input
                    type="checkbox"
                    bind:checked={settings.autostart}
                    class="toggle-input"
                    id="autostart"
                  />
                  <label for="autostart" class="toggle-slider"></label>
                </div>
              </div>
            </div>

            <div class="setting-group">
              <div class="input-setting">
                <h3>Global Access Key</h3>
                <p class="text-secondary">Hotkey combination for window activation</p>
                <input
                  class="input input-gaming"
                  bind:value={settings.global_shortcut}
                  placeholder="CTRL+F10"
                />
              </div>
            </div>
          </div>
        </div>

        <!-- Plugins -->
        <div class="card">
          <div class="card-header">
            <h2 class="h3">🔌 Plugins</h2>
            <p class="text-secondary">Subprocess target provider executables</p>
          </div>
          <div class="card-body">
            <div class="plugin-list">
              {#each settings.target_providers.plugins as plugin}
                {#if editingId === plugin.id}
                  <!-- Edit mode -->
                  <div class="plugin-item plugin-item--editing">
                    <div class="plugin-edit-fields">
                      <input class="input" bind:value={editDraft.name} placeholder="Name" />
                      <input class="input plugin-command-input" bind:value={editDraft.command} placeholder="Command" />
                    </div>
                    <div class="plugin-edit-actions">
                      <span class="test-result" class:test-ok={editTestResult === true} class:test-fail={editTestResult === false}>
                        {#if editTestResult === true}ok{:else if editTestResult === false}fail{/if}
                      </span>
                      <button type="button" class="btn btn-secondary btn-sm" on:click={testEditPlugin}>Test</button>
                      <button type="button" class="btn btn-primary btn-sm" on:click={() => saveEdit(plugin.id)}>Save</button>
                      <button type="button" class="btn btn-sm" on:click={cancelEdit}>Cancel</button>
                      <button type="button" class="btn btn-danger btn-sm" on:click={() => removePlugin(plugin.id)}>✕</button>
                    </div>
                  </div>
                {:else}
                  <!-- Read mode -->
                  <div class="plugin-item" class:plugin-item--invalid={pluginPathValid[plugin.id] === false}>
                    <div class="plugin-info">
                      <h3 class="plugin-name">
                        {plugin.name}
                        {#if pluginPathValid[plugin.id] === false}
                          <span class="badge-warning" title="Executable not found — check the command path">⚠ not found</span>
                        {/if}
                      </h3>
                      <p class="plugin-path text-secondary">{plugin.command}</p>
                    </div>
                    <div class="plugin-actions">
                      <div class="toggle-wrapper">
                        <input
                          type="checkbox"
                          checked={plugin.enabled}
                          class="toggle-input"
                          id="plugin-{plugin.id}"
                          on:change={(e) => togglePlugin(plugin.id, e.currentTarget.checked)}
                        />
                        <label for="plugin-{plugin.id}" class="toggle-slider"></label>
                      </div>
                      <button type="button" class="btn btn-secondary btn-sm" on:click={() => startEdit(plugin)}>Edit</button>
                      <button type="button" class="btn btn-danger btn-sm" on:click={() => removePlugin(plugin.id)}>✕</button>
                    </div>
                  </div>
                {/if}
              {/each}

              <!-- Add row -->
              <div class="plugin-add">
                <input class="input" bind:value={newPlugin.name} placeholder="Name" />
                <input
                  class="input plugin-command-input"
                  bind:value={newPlugin.command}
                  placeholder="Command (e.g. node C:\plugins\demo.js)"
                  on:input={() => newPluginTestResult = null}
                  on:keydown={(e) => e.key === 'Enter' && addPlugin()}
                />
                <span class="test-result" class:test-ok={newPluginTestResult === true} class:test-fail={newPluginTestResult === false}>
                  {#if newPluginTestResult === true}ok{:else if newPluginTestResult === false}fail{/if}
                </span>
                <button type="button" class="btn btn-secondary btn-sm" on:click={testNewPlugin}>Test</button>
                <button type="button" class="btn btn-secondary" on:click={addPlugin}>Add</button>
              </div>
            </div>
          </div>
        </div>

        <!-- Registry -->
        <div class="card">
          <div class="card-header">
            <h2 class="h3">📦 Plugin Registry</h2>
            <p class="text-secondary">Browse and install published plugins</p>
          </div>
          <div class="card-body">
            <div class="registry-url-row">
              <input
                class="input plugin-command-input"
                bind:value={settings.registry_url}
                placeholder="Registry URL"
              />
              <button
                type="button"
                class="btn btn-secondary btn-sm"
                on:click={loadRegistry}
                disabled={registryLoading}
              >
                {registryLoading ? '...' : 'Browse'}
              </button>
            </div>

            {#if registryError}
              <p class="registry-error">{registryError}</p>
            {/if}

            {#if registry}
              <div class="registry-list">
                {#each registry.plugins as rp}
                  <div class="registry-item">
                    <div class="plugin-info">
                      <h3 class="plugin-name">
                        {rp.name}
                        <span class="badge-version">v{rp.version}</span>
                      </h3>
                      <p class="plugin-path text-secondary">{rp.description}</p>
                      <p class="registry-author text-secondary">by {rp.author}</p>
                    </div>
                    <div class="plugin-actions">
                      {#if isInstalled(rp)}
                        <span class="badge-installed">installed</span>
                      {:else}
                        <button
                          type="button"
                          class="btn btn-primary btn-sm"
                          disabled={installingId === rp.id}
                          on:click={() => installPlugin(rp)}
                        >
                          {installingId === rp.id ? '...' : 'Install'}
                        </button>
                      {/if}
                    </div>
                  </div>
                  {#if installErrors[rp.id]}
                    <p class="install-error">{installErrors[rp.id]}</p>
                  {/if}
                {/each}
              </div>
            {/if}
          </div>
        </div>

        <!-- Regex Patterns -->
        <div class="card">
          <div class="card-header">
            <h2 class="h3">🔍 Pattern Recognition</h2>
            <p class="text-secondary">Content detection algorithms</p>
          </div>
          <div class="card-body">
            <div class="regex-list">
              {#each settings.regex_list as pattern, index}
                <div class="regex-item">
                  <span class="regex-number">{(index + 1).toString().padStart(2, '0')}.</span>
                  <input
                    bind:value={settings.regex_list[index]}
                    placeholder="Enter pattern..."
                    class="input regex-input"
                  />
                  <button
                    type="button"
                    class="btn btn-danger btn-sm"
                    on:click={() => removeRegexPattern(index)}
                  >
                    ✕
                  </button>
                </div>
              {/each}

              <button
                type="button"
                class="btn btn-secondary btn-full"
                on:click={addRegexPattern}
              >
                + Add Pattern
              </button>
            </div>
          </div>
        </div>

        <!-- Action Buttons -->
        <div class="card">
          <div class="card-body">
            <div class="actions flex gap-md">
              <button
                type="submit"
                class="btn btn-primary btn-lg"
                disabled={saving}
              >
                {#if saving}
                  <div class="spinner small"></div>
                  <span>Saving Configuration...</span>
                {:else}
                  💾 Save Configuration
                {/if}
              </button>

              <button
                type="button"
                class="btn btn-secondary"
                on:click={resetSettings}
                disabled={saving}
              >
                🔄 Reset to Defaults
              </button>
            </div>
          </div>
        </div>

      </form>
    {:else}
      <div class="card">
        <div class="error-state">
          <h2 class="text-accent">Configuration Load Failed</h2>
          <button class="btn btn-primary" on:click={loadSettings}>
            Retry Connection
          </button>
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .header-content {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--space-md) 0;
  }

  .header-main {
    flex: 1;
  }

  .close-btn-aligned {
    margin-right: var(--space-md);
  }

  .settings-form {
    display: flex;
    flex-direction: column;
    gap: var(--space-lg);
  }

  .setting-group {
    margin-bottom: var(--space-lg);
  }

  .setting-group:last-child {
    margin-bottom: 0;
  }

  .toggle-setting {
    display: flex;
    justify-content: space-between;
    align-items: center;
    cursor: pointer;
    padding: var(--space-lg);
    background: linear-gradient(135deg, var(--bg-elevated), var(--bg-surface));
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-lg);
    transition: all var(--transition-normal);
    position: relative;
    overflow: hidden;
  }

  .toggle-setting::before {
    content: '';
    position: absolute;
    top: 0;
    left: 0;
    width: 4px;
    height: 100%;
    background: linear-gradient(135deg, var(--accent-primary), var(--accent-secondary));
    opacity: 0;
    transition: opacity var(--transition-normal);
  }

  .toggle-setting:hover {
    border-color: var(--accent-primary);
    transform: translateX(4px);
  }

  .toggle-setting:hover::before {
    opacity: 1;
  }

  .setting-info h3 {
    margin: 0 0 var(--space-xs) 0;
    font-family: var(--font-gaming);
    font-size: 1.125rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .setting-info p {
    margin: 0;
    font-size: 0.875rem;
    color: var(--text-secondary);
  }

  .input-setting h3 {
    margin: 0 0 var(--space-xs) 0;
    font-family: var(--font-gaming);
    font-size: 1.125rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .input-setting p {
    margin: 0 0 var(--space-md) 0;
    font-size: 0.875rem;
    color: var(--text-secondary);
  }

  /* Plugin list */
  .plugin-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-md);
  }

  .plugin-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-md);
    padding: var(--space-md);
    background: linear-gradient(135deg, var(--bg-elevated), var(--bg-surface));
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-md);
  }

  .plugin-info {
    flex: 1;
    min-width: 0;
  }

  .plugin-name {
    margin: 0 0 2px 0;
    font-family: var(--font-gaming);
    font-size: 0.9rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .plugin-path {
    margin: 0;
    font-family: var(--font-mono);
    font-size: 0.75rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .plugin-actions {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    flex-shrink: 0;
  }

  .plugin-add {
    display: grid;
    grid-template-columns: 1fr 2fr auto auto auto;
    gap: var(--space-sm);
    align-items: center;
    padding-top: var(--space-sm);
    border-top: 1px solid var(--border-primary);
  }

  .plugin-item--editing {
    flex-direction: column;
    align-items: stretch;
    gap: var(--space-sm);
  }

  .plugin-edit-fields {
    display: grid;
    grid-template-columns: 1fr 2fr;
    gap: var(--space-sm);
  }

  .plugin-edit-actions {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    justify-content: flex-end;
  }

  .test-result {
    font-family: var(--font-mono);
    font-size: 0.75rem;
    font-weight: 600;
    min-width: 6ch;
    text-align: center;
  }

  .test-result.test-ok {
    color: #00ff88;
  }

  .test-result.test-fail {
    color: #ff5050;
  }

  .plugin-command-input {
    font-family: var(--font-mono);
    font-size: 0.8rem;
  }

  /* Regex List */
  .regex-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-md);
  }

  .regex-item {
    display: flex;
    align-items: center;
    gap: var(--space-md);
  }

  .regex-number {
    font-family: var(--font-mono);
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--accent-primary);
    min-width: 32px;
    text-shadow: 0 0 10px var(--accent-primary);
  }

  .regex-input {
    flex: 1;
    font-family: var(--font-mono);
    font-size: 0.875rem;
  }

  .actions {
    display: flex;
    gap: var(--space-md);
  }

  .actions .btn {
    flex: 1;
  }

  .error-state {
    text-align: center;
    padding: var(--space-2xl);
  }

  .error-state h2 {
    margin-bottom: var(--space-lg);
    font-family: var(--font-gaming);
  }

  .empty-state {
    text-align: center;
    padding: var(--space-lg);
  }

  .empty-icon {
    font-size: 2rem;
    margin-bottom: var(--space-sm);
    opacity: 0.6;
  }

  .registry-url-row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: var(--space-sm);
    margin-bottom: var(--space-md);
  }

  .install-error {
    color: #ff5050;
    font-size: 0.75rem;
    font-family: var(--font-mono);
    margin: var(--space-xs) var(--space-md) 0;
  }

  .registry-error {
    color: #ff5050;
    font-size: 0.8rem;
    font-family: var(--font-mono);
    margin-bottom: var(--space-sm);
  }

  .registry-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .registry-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-md);
    padding: var(--space-md);
    background: linear-gradient(135deg, var(--bg-elevated), var(--bg-surface));
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-md);
  }

  .registry-author {
    margin: 0;
    font-size: 0.7rem;
    font-family: var(--font-mono);
  }

  .badge-version {
    display: inline-block;
    font-size: 0.65rem;
    font-family: var(--font-mono);
    padding: 2px 5px;
    border-radius: 4px;
    background: rgba(0, 212, 255, 0.1);
    border: 1px solid rgba(0, 212, 255, 0.3);
    color: var(--accent-primary);
    vertical-align: middle;
    margin-left: 6px;
  }

  .badge-installed {
    font-size: 0.65rem;
    font-family: var(--font-mono);
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 4px;
    background: rgba(0, 255, 136, 0.1);
    border: 1px solid rgba(0, 255, 136, 0.3);
    color: #00ff88;
  }

  .badge-coming-soon {
    display: inline-block;
    font-size: 0.65rem;
    font-family: var(--font-mono);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 2px 6px;
    border-radius: 4px;
    background: rgba(255, 165, 0, 0.15);
    border: 1px solid rgba(255, 165, 0, 0.4);
    color: #ffa500;
    vertical-align: middle;
    margin-left: 6px;
  }

  .badge-warning {
    display: inline-block;
    font-size: 0.65rem;
    font-family: var(--font-mono);
    font-weight: 600;
    padding: 2px 6px;
    border-radius: 4px;
    background: rgba(255, 80, 80, 0.15);
    border: 1px solid rgba(255, 80, 80, 0.4);
    color: #ff5050;
    vertical-align: middle;
    margin-left: 6px;
    cursor: help;
  }

  .plugin-item--invalid {
    border-color: rgba(255, 80, 80, 0.4);
  }
</style>
