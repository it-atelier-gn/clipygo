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
    registry_id?: string;
    version?: string;
  }

  interface HistorySettings {
    enabled: boolean;
    memory_buffer_mb: number;
    persist_to_disk: boolean;
    disk_buffer_mb: number;
  }

  type MorphAction =
    | { kind: 'replace'; find: string; replace: string }
    | { kind: 'builtin'; transform: string };

  interface MorphRule {
    id: string;
    name: string;
    enabled: boolean;
    pattern: string;
    action: MorphAction;
  }

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

  const BUILTIN_TRANSFORMS: { id: string; label: string }[] = [
    { id: 'strip_tracking', label: 'Strip URL tracking' },
    { id: 'strip_html', label: 'Strip HTML tags' },
    { id: 'html_encode', label: 'HTML encode' },
    { id: 'json_pretty', label: 'JSON pretty' },
    { id: 'json_minify', label: 'JSON minify' },
    { id: 'xml_pretty', label: 'XML pretty' },
    { id: 'base64_encode', label: 'Base64 encode' },
    { id: 'base64_decode', label: 'Base64 decode' },
    { id: 'url_encode', label: 'URL encode' },
    { id: 'url_decode', label: 'URL decode' },
    { id: 'uppercase', label: 'UPPERCASE' },
    { id: 'lowercase', label: 'lowercase' },
    { id: 'title_case', label: 'Title Case' },
    { id: 'snake_case', label: 'snake_case' },
    { id: 'camel_case', label: 'camelCase' },
    { id: 'kebab_case', label: 'kebab-case' },
    { id: 'slugify', label: 'Slugify' },
    { id: 'remove_diacritics', label: 'Remove accents' },
    { id: 'straighten_quotes', label: 'Straighten quotes/dashes' },
    { id: 'trim', label: 'Trim whitespace' },
    { id: 'collapse_whitespace', label: 'Collapse whitespace' },
    { id: 'trim_lines', label: 'Trim each line' },
    { id: 'normalize_newlines', label: 'Normalize newlines (LF)' },
    { id: 'sort_lines', label: 'Sort lines' },
    { id: 'reverse_lines', label: 'Reverse lines' },
    { id: 'dedupe_lines', label: 'Dedupe lines' },
    { id: 'remove_empty_lines', label: 'Remove empty lines' },
  ];

  interface AppSettings {
    autostart: boolean;
    global_shortcut: string;
    history_shortcut: string;
    regex_list: string[];
    target_providers: {
      msteams: { enabled: boolean };
      plugins: PluginProvider[];
    };
    registry_url: string;
    show_debug_log: boolean;
    history: HistorySettings;
    morph_enabled: boolean;
    morph_shortcut: string;
    morph_rules: MorphRule[];
    exec_enabled: boolean;
    exec_shortcut: string;
    exec_commands: ExecCommand[];
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
  let message = "";
  let messageType: 'success' | 'error' | '' = '';
  let newPlugin = { name: '', command: '' };
  let newPluginTestResult: boolean | null = null;
  let pluginPathValid: Record<string, boolean> = {};
  let pluginLinks: Record<string, string> = {};
  let pluginStatuses: Record<string, { healthy: boolean; error: string | null }> = {};

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

  function getInstalledVersion(plugin: RegistryPlugin): string | undefined {
    return settings?.target_providers.plugins.find(p => p.registry_id === plugin.id)?.version ?? undefined;
  }

  function isOutdated(plugin: RegistryPlugin): boolean {
    const installed = getInstalledVersion(plugin);
    return installed !== undefined && installed !== plugin.version;
  }

  let updatingId: string | null = null;
  let updateErrors: Record<string, string> = {};

  async function updatePlugin(plugin: RegistryPlugin) {
    const platform = detectPlatform();
    if (!plugin.platforms[platform]) {
      updateErrors = { ...updateErrors, [plugin.id]: `No binary available for platform '${platform}'` };
      return;
    }
    updatingId = plugin.id;
    updateErrors = { ...updateErrors, [plugin.id]: '' };
    try {
      await invoke('update_registry_plugin', { plugin, platformKey: platform });
      await loadSettings();
      showMessage(`${plugin.name} updated to v${plugin.version}`, 'success');
    } catch (e) {
      updateErrors = { ...updateErrors, [plugin.id]: `${e}` };
    } finally {
      updatingId = null;
    }
  }

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
      fetchPluginLinks();
      fetchPluginStatuses();
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

  async function fetchPluginLinks() {
    if (!settings) return;
    const results = await Promise.all(
      settings.target_providers.plugins.filter(p => p.enabled).map(async (p) => {
        try {
          const link: string | null = await invoke('get_plugin_link', { pluginId: p.id });
          return [p.id, link ?? ''] as [string, string];
        } catch {
          return [p.id, ''] as [string, string];
        }
      })
    );
    pluginLinks = Object.fromEntries(results.filter(([, link]) => link));
  }

  async function fetchPluginStatuses() {
    try {
      const entries: { id: string; name: string; healthy: boolean; error: string | null }[] =
        await invoke('get_plugin_statuses');
      pluginStatuses = Object.fromEntries(
        entries.map(e => [e.id, { healthy: e.healthy, error: e.error }])
      );
    } catch {
    }
  }

  let settingsSaveTimer: ReturnType<typeof setTimeout> | null = null;

  function scheduleSave() {
    if (settingsSaveTimer) clearTimeout(settingsSaveTimer);
    settingsSaveTimer = setTimeout(async () => {
      if (!settings) return;
      try {
        await invoke('save_settings', { settings });
      } catch (error) {
        showMessage('Failed to save settings', 'error');
      }
    }, 500);
  }

  function addRegexPattern() {
    if (settings) {
      settings.regex_list = [...settings.regex_list, ''];
      scheduleSave();
    }
  }

  function removeRegexPattern(index: number) {
    if (settings) {
      settings.regex_list = settings.regex_list.filter((_, i) => i !== index);
      scheduleSave();
    }
  }

  function addMorphRule() {
    if (!settings) return;
    const rule: MorphRule = {
      id: crypto.randomUUID(),
      name: 'New rule',
      enabled: false,
      pattern: '',
      action: { kind: 'builtin', transform: 'trim' },
    };
    settings.morph_rules = [...settings.morph_rules, rule];
    scheduleSave();
  }

  function removeMorphRule(index: number) {
    if (!settings) return;
    settings.morph_rules = settings.morph_rules.filter((_, i) => i !== index);
    scheduleSave();
  }

  function setMorphActionKind(index: number, kind: 'replace' | 'builtin') {
    if (!settings) return;
    const rule = settings.morph_rules[index];
    if (rule.action.kind === kind) return;
    rule.action =
      kind === 'replace'
        ? { kind: 'replace', find: '', replace: '' }
        : { kind: 'builtin', transform: 'trim' };
    settings.morph_rules = settings.morph_rules;
    scheduleSave();
  }

  interface MorphTestResult {
    matched: boolean;
    changed: boolean;
    output: string;
    error: string | null;
  }

  interface MorphTestState {
    input: string;
    open: boolean;
    result: MorphTestResult | null;
  }

  let morphTests: Record<string, MorphTestState> = {};
  let morphTestTimers: Record<string, ReturnType<typeof setTimeout>> = {};

  function ensureTest(id: string): MorphTestState {
    if (!morphTests[id]) morphTests[id] = { input: '', open: false, result: null };
    return morphTests[id];
  }

  function toggleTest(rule: MorphRule) {
    const t = ensureTest(rule.id);
    t.open = !t.open;
    morphTests = morphTests;
    if (t.open && t.input) runMorphTest(rule);
  }

  async function runMorphTest(rule: MorphRule) {
    const t = ensureTest(rule.id);
    try {
      t.result = await invoke<MorphTestResult>('morph_test_rule', { rule, input: t.input });
    } catch (e) {
      t.result = { matched: false, changed: false, output: '', error: String(e) };
    }
    morphTests = morphTests;
  }

  function onTestInput(rule: MorphRule, value: string) {
    const t = ensureTest(rule.id);
    t.input = value;
    morphTests = morphTests;
    clearTimeout(morphTestTimers[rule.id]);
    morphTestTimers[rule.id] = setTimeout(() => runMorphTest(rule), 150);
  }

  function addExecCommand() {
    if (!settings) return;
    const cmd: ExecCommand = {
      id: crypto.randomUUID(),
      name: 'New command',
      enabled: false,
      pattern: '',
      path: '',
      args: '',
      working_dir: '',
      pipe_stdin: false,
    };
    settings.exec_commands = [...settings.exec_commands, cmd];
    scheduleSave();
  }

  function removeExecCommand(index: number) {
    if (!settings) return;
    settings.exec_commands = settings.exec_commands.filter((_, i) => i !== index);
    scheduleSave();
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


  interface ArrayItemProperty {
    type: 'string';
    title?: string;
    description?: string;
    format?: string;
  }

  interface ArrayItems {
    type: 'object';
    properties: Record<string, ArrayItemProperty>;
    required?: string[];
  }

  interface SchemaProperty {
    type: 'string' | 'boolean' | 'array';
    title?: string;
    description?: string;
    default?: unknown;
    format?: string;
    enum?: string[];
    enumTitles?: string[];
    visibleIf?: Record<string, string | string[]>;
    readOnly?: boolean;
    items?: ArrayItems;
    maxItems?: number;
  }

  interface ConfigSchema {
    type: 'object';
    title?: string;
    properties: Record<string, SchemaProperty>;
    required?: string[];
  }

  let configModalOpen = false;
  let configPluginId = '';
  let configPluginName = '';
  let configLoading = false;
  let configSchema: ConfigSchema | null = null;
  let configValues: Record<string, unknown> = {};
  let configInstructions = '';
  let configError = '';
  let visiblePasswords: Record<string, boolean> = {};
  let configSaveTimer: ReturnType<typeof setTimeout> | null = null;

  async function openConfigModal(plugin: PluginProvider) {
    configPluginId = plugin.id;
    configPluginName = plugin.name;
    configLoading = true;
    configSchema = null;
    configValues = {};
    configInstructions = '';
    configError = '';
    visiblePasswords = {};
    configModalOpen = true;
    try {
      const result: { schema: ConfigSchema; values: Record<string, unknown>; instructions?: string } =
        await invoke('get_plugin_config_schema', { pluginId: plugin.id });
      configSchema = result.schema;
      configValues = { ...result.values };
      configInstructions = result.instructions ?? '';
    } catch (e) {
      configError = `${e}`;
    } finally {
      configLoading = false;
    }
  }

  async function flushConfigSave() {
    if (!configSchema || !configPluginId) return;
    if (configSaveTimer) {
      clearTimeout(configSaveTimer);
      configSaveTimer = null;
    }
    const writableValues = Object.fromEntries(
      Object.entries(configValues).filter(([k]) => !configSchema?.properties[k]?.readOnly)
    );
    try {
      await invoke('set_plugin_config', { pluginId: configPluginId, values: writableValues });
    } catch (e) {
      configError = `${e}`;
    }
  }

  function scheduleConfigSave() {
    if (configSaveTimer) clearTimeout(configSaveTimer);
    configSaveTimer = setTimeout(flushConfigSave, 500);
  }

  async function closeConfigModal() {
    await flushConfigSave();
    configModalOpen = false;
  }

  async function copyToClipboard(value: string) {
    try {
      await navigator.clipboard.writeText(value);
      showMessage('Copied', 'success');
    } catch {
      showMessage('Copy failed', 'error');
    }
  }

  function setConfigValue(key: string, value: unknown) {
    configValues = { ...configValues, [key]: value };
    scheduleConfigSave();
  }

  function addArrayItem(key: string, prop: SchemaProperty) {
    const arr = [...((configValues[key] as Record<string, unknown>[]) ?? [])];
    const newItem: Record<string, unknown> = {};
    for (const field of Object.keys(prop.items?.properties ?? {})) {
      newItem[field] = '';
    }
    setConfigValue(key, [...arr, newItem]);
  }

  function removeArrayItem(key: string, index: number) {
    const arr = [...((configValues[key] as Record<string, unknown>[]) ?? [])];
    arr.splice(index, 1);
    setConfigValue(key, arr);
  }

  function updateArrayItem(key: string, index: number, field: string, value: unknown) {
    const arr = [...((configValues[key] as Record<string, unknown>[]) ?? [])];
    arr[index] = { ...arr[index], [field]: value };
    setConfigValue(key, arr);
  }
</script>

<svelte:window on:keydown={(e) => { if (e.key === 'Escape') { if (configModalOpen) closeConfigModal(); else closeWindow(); } }} />

<div class="app" data-tauri-drag-region>
  <div class="container">
    <div class="header compact">
      <div class="header-content flex justify-between items-center">
        <div class="header-main" data-tauri-drag-region>
          <h1 data-tauri-drag-region>Settings</h1>
          <p class="subtitle" data-tauri-drag-region>clipygo</p>
        </div>
        <button class="btn btn-ghost btn-icon" aria-label="Close" on:click={closeWindow}>
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
          <span>Loading settings…</span>
        </div>
      </div>
    {:else if settings}
      <div class="settings-form">

        <div class="card">
          <div class="card-header">
            <h2 class="h3">General</h2>
            <p class="text-secondary">Startup and hotkeys</p>
          </div>
          <div class="card-body">
            <div class="setting-group">
              <div class="toggle-setting">
                <div class="setting-info">
                  <h3>Start with system</h3>
                  <p class="text-secondary">Launch clipygo when you sign in</p>
                </div>
                <div class="toggle-wrapper">
                  <input
                    type="checkbox"
                    checked={settings.autostart}
                    class="toggle-input"
                    id="autostart"
                    on:change={(e) => { settings!.autostart = e.currentTarget.checked; scheduleSave(); }}
                  />
                  <label for="autostart" class="toggle-slider"></label>
                </div>
              </div>
            </div>

            <div class="setting-group">
              <div class="toggle-setting">
                <div class="setting-info">
                  <h3>Debug log</h3>
                  <p class="text-secondary">Show debug log in system tray menu (restart required)</p>
                </div>
                <div class="toggle-wrapper">
                  <input
                    type="checkbox"
                    checked={settings.show_debug_log}
                    class="toggle-input"
                    id="show_debug_log"
                    on:change={(e) => { settings!.show_debug_log = e.currentTarget.checked; scheduleSave(); }}
                  />
                  <label for="show_debug_log" class="toggle-slider"></label>
                </div>
              </div>
            </div>

            <div class="setting-group">
              <div class="input-setting">
                <h3>Main hotkey</h3>
                <p class="text-secondary">Opens the main window</p>
                <input
                  class="input input-gaming"
                  value={settings.global_shortcut}
                  placeholder="CTRL+F10"
                  on:input={(e) => { settings!.global_shortcut = e.currentTarget.value; scheduleSave(); }}
                />
              </div>
            </div>

            <div class="setting-group">
              <div class="input-setting">
                <h3>History hotkey</h3>
                <p class="text-secondary">Hotkey to open the clipboard history window</p>
                <input
                  class="input input-gaming"
                  value={settings.history_shortcut}
                  placeholder="CTRL+SHIFT+H"
                  on:input={(e) => { settings!.history_shortcut = e.currentTarget.value; scheduleSave(); }}
                />
              </div>
            </div>
          </div>
        </div>

        <div class="card">
          <div class="card-header">
            <h2 class="h3">Clipboard history</h2>
            <p class="text-secondary">Captured clipboard items, viewable only inside clipygo</p>
          </div>
          <div class="card-body">
            <div class="setting-group">
              <div class="toggle-setting">
                <div class="setting-info">
                  <h3>Capture clipboard</h3>
                  <p class="text-secondary">Record every clipboard change into an encrypted history</p>
                </div>
                <div class="toggle-wrapper">
                  <input
                    type="checkbox"
                    checked={settings.history.enabled}
                    class="toggle-input"
                    id="history_enabled"
                    on:change={(e) => { settings!.history.enabled = e.currentTarget.checked; scheduleSave(); }}
                  />
                  <label for="history_enabled" class="toggle-slider"></label>
                </div>
              </div>
            </div>

            <div class="setting-group">
              <div class="input-setting">
                <h3>Memory buffer (MB)</h3>
                <p class="text-secondary">Cap for the always-on in-memory ring buffer</p>
                <input
                  class="input input-gaming"
                  type="number"
                  min="1"
                  max="2048"
                  value={settings.history.memory_buffer_mb}
                  on:input={(e) => { settings!.history.memory_buffer_mb = Math.max(1, parseInt(e.currentTarget.value, 10) || 1); scheduleSave(); }}
                />
              </div>
            </div>

            <div class="setting-group">
              <div class="toggle-setting">
                <div class="setting-info">
                  <h3>Keep history on disk</h3>
                  <p class="text-secondary">
                    Survive restarts via encrypted SQLite. Content bytes encrypted with
                    XChaCha20-Poly1305; key in OS keystore. Toggling discards current entries.
                  </p>
                </div>
                <div class="toggle-wrapper">
                  <input
                    type="checkbox"
                    checked={settings.history.persist_to_disk}
                    class="toggle-input"
                    id="history_persist"
                    on:change={(e) => { settings!.history.persist_to_disk = e.currentTarget.checked; scheduleSave(); }}
                  />
                  <label for="history_persist" class="toggle-slider"></label>
                </div>
              </div>
            </div>

            <div class="setting-group">
              <div class="input-setting">
                <h3>Disk buffer (MB)</h3>
                <p class="text-secondary">Cap for the encrypted on-disk store (applies when persistence is on)</p>
                <input
                  class="input input-gaming"
                  type="number"
                  min="1"
                  max="65536"
                  value={settings.history.disk_buffer_mb}
                  on:input={(e) => { settings!.history.disk_buffer_mb = Math.max(1, parseInt(e.currentTarget.value, 10) || 1); scheduleSave(); }}
                />
              </div>
            </div>
          </div>
        </div>

        <div class="card">
          <div class="card-header">
            <h2 class="h3">Morph</h2>
            <p class="text-secondary">Rule-based, in-place clipboard transformations</p>
          </div>
          <div class="card-body">
            <div class="setting-group">
              <div class="toggle-setting">
                <div class="setting-info">
                  <h3>Enable Morph</h3>
                  <p class="text-secondary">Apply matching rules automatically and rewrite the clipboard in place</p>
                </div>
                <div class="toggle-wrapper">
                  <input
                    type="checkbox"
                    checked={settings.morph_enabled}
                    class="toggle-input"
                    id="morph_enabled"
                    on:change={(e) => { settings!.morph_enabled = e.currentTarget.checked; scheduleSave(); }}
                  />
                  <label for="morph_enabled" class="toggle-slider"></label>
                </div>
              </div>
            </div>

            <div class="setting-group">
              <div class="input-setting">
                <h3>Transform hotkey</h3>
                <p class="text-secondary">Opens a preview window to apply any transformation on demand</p>
                <input
                  class="input input-gaming"
                  value={settings.morph_shortcut}
                  placeholder="CTRL+SHIFT+M"
                  on:input={(e) => { settings!.morph_shortcut = e.currentTarget.value; scheduleSave(); }}
                />
              </div>
            </div>

            <div class="morph-rules">
              {#each settings.morph_rules as rule, index (rule.id)}
                <div class="morph-rule">
                  <div class="morph-rule-top">
                    <div class="toggle-wrapper">
                      <input
                        type="checkbox"
                        checked={rule.enabled}
                        class="toggle-input"
                        id="morph-rule-{rule.id}"
                        on:change={(e) => { settings!.morph_rules[index].enabled = e.currentTarget.checked; scheduleSave(); }}
                      />
                      <label for="morph-rule-{rule.id}" class="toggle-slider"></label>
                    </div>
                    <input
                      class="input morph-name"
                      value={rule.name}
                      placeholder="Rule name"
                      on:input={(e) => { settings!.morph_rules[index].name = e.currentTarget.value; scheduleSave(); }}
                    />
                    <select
                      class="input morph-kind"
                      value={rule.action.kind}
                      on:change={(e) => setMorphActionKind(index, e.currentTarget.value === 'replace' ? 'replace' : 'builtin')}
                    >
                      <option value="builtin">Built-in</option>
                      <option value="replace">Regex replace</option>
                    </select>
                    <button type="button" class="btn btn-ghost btn-icon btn-remove" aria-label="Remove" on:click={() => removeMorphRule(index)}>✕</button>
                  </div>

                  <div class="morph-rule-fields">
                    <span class="morph-field-label">Match pattern (regex)</span>
                    <input
                      class="input morph-mono"
                      value={rule.pattern}
                      placeholder="e.g. ^https?://"
                      on:input={(e) => { settings!.morph_rules[index].pattern = e.currentTarget.value; scheduleSave(); }}
                    />

                    {#if rule.action.kind === 'builtin'}
                      <span class="morph-field-label">Transform</span>
                      <select
                        class="input"
                        value={rule.action.transform}
                        on:change={(e) => { (settings!.morph_rules[index].action as { kind: 'builtin'; transform: string }).transform = e.currentTarget.value; scheduleSave(); }}
                      >
                        {#each BUILTIN_TRANSFORMS as t}
                          <option value={t.id}>{t.label}</option>
                        {/each}
                      </select>
                    {:else}
                      <span class="morph-field-label">Find (regex)</span>
                      <input
                        class="input morph-mono"
                        value={rule.action.find}
                        placeholder="^(\S+),\s+(\S+)$"
                        on:input={(e) => { (settings!.morph_rules[index].action as { kind: 'replace'; find: string; replace: string }).find = e.currentTarget.value; scheduleSave(); }}
                      />
                      <span class="morph-field-label">Replace (use $1, $2 for groups)</span>
                      <input
                        class="input morph-mono"
                        value={rule.action.replace}
                        placeholder="$2 $1"
                        on:input={(e) => { (settings!.morph_rules[index].action as { kind: 'replace'; find: string; replace: string }).replace = e.currentTarget.value; scheduleSave(); }}
                      />
                    {/if}
                  </div>

                  <div class="morph-test">
                    <button type="button" class="morph-test-toggle" on:click={() => toggleTest(rule)}>
                      {morphTests[rule.id]?.open ? '▾' : '▸'} Test rule
                    </button>
                    {#if morphTests[rule.id]?.open}
                      <textarea
                        class="input morph-mono morph-test-input"
                        rows="3"
                        placeholder="Paste sample text to test this rule against…"
                        value={morphTests[rule.id]?.input ?? ''}
                        on:input={(e) => onTestInput(rule, e.currentTarget.value)}
                      ></textarea>
                      {#if morphTests[rule.id]?.result}
                        {@const res = morphTests[rule.id].result!}
                        {#if res.error}
                          <div class="morph-test-status err">⚠ {res.error}</div>
                        {:else}
                          <div class="morph-test-status">
                            <span class="badge {res.matched ? 'badge-ok' : 'badge-muted'}">
                              {res.matched ? 'Pattern matches' : 'No match'}
                            </span>
                            {#if res.matched}
                              <span class="badge {res.changed ? 'badge-ok' : 'badge-muted'}">
                                {res.changed ? 'Output changed' : 'No change'}
                              </span>
                            {/if}
                          </div>
                          {#if res.matched}
                            <span class="morph-field-label">Result</span>
                            <pre class="morph-test-output">{res.output}</pre>
                          {/if}
                        {/if}
                      {/if}
                    {/if}
                  </div>
                </div>
              {/each}

              <button type="button" class="btn btn-secondary btn-full" on:click={addMorphRule}>
                + Add rule
              </button>
            </div>
          </div>
        </div>

        <div class="card">
          <div class="card-header">
            <h2 class="h3">Execute</h2>
            <p class="text-secondary">Launch external commands from the clipboard</p>
          </div>
          <div class="card-body">
            <div class="setting-group">
              <div class="toggle-setting">
                <div class="setting-info">
                  <h3>Auto-run on match</h3>
                  <p class="text-secondary">When the hotkey is pressed and exactly one command with a (non-empty) match pattern matches the clipboard, run it directly instead of opening the picker. Commands without a pattern never auto-run — they only appear in the picker.</p>
                </div>
                <div class="toggle-wrapper">
                  <input
                    type="checkbox"
                    checked={settings.exec_enabled}
                    class="toggle-input"
                    id="exec_enabled"
                    on:change={(e) => { settings!.exec_enabled = e.currentTarget.checked; scheduleSave(); }}
                  />
                  <label for="exec_enabled" class="toggle-slider"></label>
                </div>
              </div>
            </div>

            <div class="setting-group">
              <div class="input-setting">
                <h3>Execute hotkey</h3>
                <p class="text-secondary">Opens the command picker (or auto-runs a single matching command)</p>
                <input
                  class="input input-gaming"
                  value={settings.exec_shortcut}
                  placeholder="CTRL+SHIFT+E"
                  on:input={(e) => { settings!.exec_shortcut = e.currentTarget.value; scheduleSave(); }}
                />
              </div>
            </div>

            <div class="morph-rules">
              {#each settings.exec_commands as cmd, index (cmd.id)}
                <div class="morph-rule">
                  <div class="morph-rule-top">
                    <div class="toggle-wrapper">
                      <input
                        type="checkbox"
                        checked={cmd.enabled}
                        class="toggle-input"
                        id="exec-cmd-{cmd.id}"
                        on:change={(e) => { settings!.exec_commands[index].enabled = e.currentTarget.checked; scheduleSave(); }}
                      />
                      <label for="exec-cmd-{cmd.id}" class="toggle-slider"></label>
                    </div>
                    <input
                      class="input morph-name"
                      value={cmd.name}
                      placeholder="Command name"
                      on:input={(e) => { settings!.exec_commands[index].name = e.currentTarget.value; scheduleSave(); }}
                    />
                    <button type="button" class="btn btn-ghost btn-icon btn-remove" aria-label="Remove" on:click={() => removeExecCommand(index)}>✕</button>
                  </div>

                  <div class="morph-rule-fields">
                    <span class="morph-field-label">Executable path</span>
                    <input
                      class="input morph-mono"
                      value={cmd.path}
                      placeholder="e.g. C:\Windows\System32\notepad.exe"
                      on:input={(e) => { settings!.exec_commands[index].path = e.currentTarget.value; scheduleSave(); }}
                    />

                    <span class="morph-field-label">Arguments (use &#123;clipboard&#125; to insert clipboard text)</span>
                    <input
                      class="input morph-mono"
                      value={cmd.args}
                      placeholder={'--input {clipboard}'}
                      on:input={(e) => { settings!.exec_commands[index].args = e.currentTarget.value; scheduleSave(); }}
                    />

                    <span class="morph-field-label">Working directory (optional)</span>
                    <input
                      class="input morph-mono"
                      value={cmd.working_dir}
                      placeholder="e.g. C:\Projects"
                      on:input={(e) => { settings!.exec_commands[index].working_dir = e.currentTarget.value; scheduleSave(); }}
                    />

                    <span class="morph-field-label">Match pattern (regex, optional — empty = always in picker, never auto-runs)</span>
                    <input
                      class="input morph-mono"
                      value={cmd.pattern}
                      placeholder="e.g. ^https?://"
                      on:input={(e) => { settings!.exec_commands[index].pattern = e.currentTarget.value; scheduleSave(); }}
                    />

                    <label class="exec-check">
                      <input
                        type="checkbox"
                        checked={cmd.pipe_stdin}
                        on:change={(e) => { settings!.exec_commands[index].pipe_stdin = e.currentTarget.checked; scheduleSave(); }}
                      />
                      Pipe clipboard to the command's standard input
                    </label>
                  </div>
                </div>
              {/each}

              <button type="button" class="btn btn-secondary btn-full" on:click={addExecCommand}>
                + Add command
              </button>
            </div>
          </div>
        </div>

        <div class="card">
          <div class="card-header">
            <h2 class="h3">Plugins</h2>
            <p class="text-secondary">Programs that provide send targets</p>
          </div>
          <div class="card-body">
            <div class="plugin-list">
              {#each settings.target_providers.plugins as plugin}
                {#if editingId === plugin.id}
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
                      <button type="button" class="btn btn-ghost btn-sm" on:click={cancelEdit}>Cancel</button>
                      <button type="button" class="btn btn-ghost btn-icon btn-remove" aria-label="Remove" on:click={() => removePlugin(plugin.id)}>✕</button>
                    </div>
                  </div>
                {:else}
                  <div class="plugin-item" class:plugin-item--invalid={pluginPathValid[plugin.id] === false || (pluginStatuses[plugin.id] && !pluginStatuses[plugin.id].healthy)}>
                    <div class="plugin-info">
                      <h3 class="plugin-name">
                        {plugin.name}
                        {#if plugin.version}
                          <span class="badge-version">v{plugin.version}</span>
                        {/if}
                        {#if pluginLinks[plugin.id]}
                          <a href={pluginLinks[plugin.id]} target="_blank" rel="noopener" class="plugin-link" title="Open plugin page">↗</a>
                        {/if}
                        {#if pluginPathValid[plugin.id] === false}
                          <span class="badge-warning" title="Executable not found — check the command path">⚠ not found</span>
                        {:else if pluginStatuses[plugin.id] && !pluginStatuses[plugin.id].healthy}
                          <span class="badge-warning" title={pluginStatuses[plugin.id].error ?? 'Plugin is in error state'}>⚠ error</span>
                        {:else if pluginStatuses[plugin.id]?.healthy}
                          <span class="badge-healthy" title="Plugin is running">● ok</span>
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
                      <button type="button" class="btn btn-secondary btn-sm" title="Configure plugin" on:click={() => openConfigModal(plugin)}>⚙</button>
                      <button type="button" class="btn btn-secondary btn-sm" on:click={() => startEdit(plugin)}>Edit</button>
                      <button type="button" class="btn btn-ghost btn-icon btn-remove" aria-label="Remove" on:click={() => removePlugin(plugin.id)}>✕</button>
                    </div>
                  </div>
                {/if}
              {/each}

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

        <div class="card">
          <div class="card-header">
            <h2 class="h3">Plugin registry</h2>
            <p class="text-secondary">Browse and install published plugins</p>
          </div>
          <div class="card-body">
            <div class="registry-url-row">
              <input
                class="input plugin-command-input"
                value={settings.registry_url}
                placeholder="Registry URL"
                on:input={(e) => { settings!.registry_url = e.currentTarget.value; scheduleSave(); }}
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
                      {#if isOutdated(rp)}
                        <button
                          type="button"
                          class="badge-update"
                          disabled={updatingId === rp.id}
                          on:click={() => updatePlugin(rp)}
                          title="Click to update"
                        >
                          {updatingId === rp.id ? '...' : `v${getInstalledVersion(rp)} → v${rp.version}`}
                        </button>
                      {:else if isInstalled(rp)}
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
                  {#if updateErrors[rp.id]}
                    <p class="install-error">{updateErrors[rp.id]}</p>
                  {/if}
                {/each}
              </div>
            {/if}
          </div>
        </div>

        <div class="card">
          <div class="card-header">
            <h2 class="h3">Patterns</h2>
            <p class="text-secondary">Regular expressions that mark clipboard content as a match</p>
          </div>
          <div class="card-body">
            <div class="regex-list">
              {#each settings.regex_list as pattern, index}
                <div class="regex-item">
                  <span class="regex-number">{(index + 1).toString().padStart(2, '0')}.</span>
                  <input
                    value={pattern}
                    placeholder="Regular expression"
                    class="input regex-input"
                    on:input={(e) => { settings!.regex_list[index] = e.currentTarget.value; scheduleSave(); }}
                  />
                  <button
                    type="button"
                    class="btn btn-ghost btn-icon btn-remove"
                    aria-label="Remove"
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
                + Add pattern
              </button>
            </div>
          </div>
        </div>

      </div>
    {:else}
      <div class="card">
        <div class="error-state">
          <h2>Settings could not be loaded</h2>
          <button class="btn btn-primary" on:click={loadSettings}>
            Retry
          </button>
        </div>
      </div>
    {/if}
  </div>
</div>

{#if configModalOpen}
  <!-- svelte-ignore a11y-click-events-have-key-events a11y-no-static-element-interactions -->
  <div class="modal-overlay" on:click|self={closeConfigModal}>
    <div class="modal">
      <div class="modal-header">
        <h2 class="modal-title">{configPluginName}</h2>
        <button class="btn btn-ghost btn-icon" aria-label="Close" on:click={closeConfigModal}>✕</button>
      </div>
      <div class="modal-body">
        {#if configLoading}
          <div class="loading-state">
            <div class="spinner"></div>
            <span>Loading configuration...</span>
          </div>
        {:else if configError && !configSchema}
          <p class="config-error">{configError}</p>
        {:else if configSchema}
          {#if configInstructions}
            <div class="config-instructions">{configInstructions}</div>
          {/if}
          <div class="config-fields">
            {#each Object.entries(configSchema.properties) as [key, prop]}
              {@const visible = !prop.visibleIf || Object.entries(prop.visibleIf).every(([field, expected]) => {
                const val = String(configValues[field] ?? '');
                return Array.isArray(expected) ? expected.includes(val) : val === expected;
              })}
              {#if visible}
              <div class="config-field">
                <label class="config-label" for="cfg-{key}">
                  {prop.title ?? key}{configSchema.required?.includes(key) ? ' *' : ''}
                </label>
                {#if prop.description}
                  <p class="config-description">{prop.description}</p>
                {/if}
                {#if prop.type === 'boolean'}
                  <div class="toggle-wrapper">
                    <input
                      type="checkbox"
                      id="cfg-{key}"
                      class="toggle-input"
                      checked={!!configValues[key]}
                      on:change={(e) => setConfigValue(key, e.currentTarget.checked)}
                    />
                    <label for="cfg-{key}" class="toggle-slider"></label>
                  </div>
                {:else if prop.type === 'array' && prop.items?.type === 'object'}
                  <div class="array-field">
                    {#each (configValues[key] as Record<string, unknown>[]) ?? [] as item, i}
                      <div class="array-item">
                        {#each Object.entries(prop.items.properties) as [field, fieldProp]}
                          <input
                            class="input array-item-input"
                            type="text"
                            placeholder={fieldProp.title ?? field}
                            value={String(item[field] ?? '')}
                            on:input={(e) => updateArrayItem(key, i, field, e.currentTarget.value)}
                          />
                        {/each}
                        <button type="button" class="btn btn-ghost btn-sm btn-remove" on:click={() => removeArrayItem(key, i)}>Remove</button>
                      </div>
                    {/each}
                    {#if !prop.maxItems || ((configValues[key] as unknown[])?.length ?? 0) < prop.maxItems}
                      <button type="button" class="btn btn-sm btn-secondary" on:click={() => addArrayItem(key, prop)}>+ Add</button>
                    {/if}
                  </div>
                {:else if prop.enum}
                  <select
                    id="cfg-{key}"
                    class="input"
                    on:change={(e) => setConfigValue(key, e.currentTarget.value)}
                  >
                    {#each prop.enum as opt, i}
                      <option value={opt} selected={String(configValues[key] ?? prop.default ?? '') === opt}>
                        {prop.enumTitles?.[i] ?? opt}
                      </option>
                    {/each}
                  </select>
                {:else if prop.format === 'password'}
                  <div class="password-wrapper">
                    <input
                      id="cfg-{key}"
                      class="input"
                      type={visiblePasswords[key] ? 'text' : 'password'}
                      value={String(configValues[key] ?? '')}
                      readonly={prop.readOnly ?? false}
                      on:input={(e) => { if (!prop.readOnly) setConfigValue(key, e.currentTarget.value); }}
                    />
                    {#if prop.readOnly}
                      <button type="button" class="copy-btn" title="Copy" on:click={() => copyToClipboard(String(configValues[key] ?? ''))}>📋</button>
                    {/if}
                    <button
                      type="button"
                      class="eye-toggle"
                      title={visiblePasswords[key] ? 'Hide' : 'Show'}
                      on:click={() => visiblePasswords[key] = !visiblePasswords[key]}
                    >
                      {visiblePasswords[key] ? '🙈' : '👁'}
                    </button>
                  </div>
                {:else if prop.readOnly}
                  <div class="readonly-wrapper">
                    <input
                      id="cfg-{key}"
                      class="input"
                      type="text"
                      value={String(configValues[key] ?? '')}
                      readonly
                    />
                    <button type="button" class="copy-btn" title="Copy" on:click={() => copyToClipboard(String(configValues[key] ?? ''))}>📋</button>
                  </div>
                {:else}
                  <input
                    id="cfg-{key}"
                    class="input"
                    type="text"
                    value={String(configValues[key] ?? '')}
                    on:input={(e) => setConfigValue(key, e.currentTarget.value)}
                  />
                {/if}
              </div>
              {/if}
            {/each}
          </div>
          {#if configError}
            <p class="config-error">{configError}</p>
          {/if}
        {/if}
      </div>
      <div class="modal-footer">
        <button class="btn btn-secondary" on:click={closeConfigModal}>Close</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .app {
    background: var(--bg-primary);
  }

  .container {
    max-width: 760px;
    padding: 0 var(--space-xl) var(--space-xl);
  }

  .header {
    position: sticky;
    top: 0;
    z-index: 20;
    margin: 0 calc(-1 * var(--space-xl)) var(--space-lg);
    padding: 0 var(--space-sm) 0 var(--space-xl);
  }

  .header-content {
    display: flex;
    justify-content: space-between;
    align-items: center;
    height: 44px;
    padding: 0;
  }

  .header-main {
    flex: 1;
    display: flex;
    align-items: baseline;
    gap: var(--space-md);
  }

  .header-main h1 {
    font-family: var(--font-serif);
    font-size: 1.15rem;
    font-weight: 500;
    margin: 0;
  }

  .header-main .subtitle {
    font-size: 0.78rem;
  }

  .message {
    margin-bottom: var(--space-md);
  }

  .settings-form {
    display: flex;
    flex-direction: column;
    gap: var(--space-lg);
  }

  .card-header {
    display: flex;
    align-items: baseline;
    gap: var(--space-md);
    padding: var(--space-md) var(--space-lg);
    background: var(--bg-secondary);
  }

  .card-header h2 {
    font-family: var(--font-serif);
    font-size: 0.98rem;
    font-weight: 500;
    margin: 0;
  }

  .card-header p {
    font-size: 0.75rem;
    color: var(--text-muted);
  }

  .card-body {
    padding: 0 var(--space-lg);
  }

  .card-body > .morph-rules,
  .card-body > .plugin-list,
  .card-body > .regex-list,
  .card-body > .registry-url-row,
  .card-body > .registry-list {
    padding: var(--space-md) 0;
  }

  .card-body > .registry-url-row + .registry-list {
    padding-top: 0;
  }

  .setting-group {
    border-bottom: 1px solid var(--border-primary);
  }

  .setting-group:last-child {
    border-bottom: none;
  }

  .toggle-setting {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-xl);
    padding: var(--space-md) 0;
  }

  .setting-info h3,
  .input-setting h3 {
    margin: 0;
    font-size: 0.85rem;
    font-weight: 500;
    color: var(--text-primary);
  }

  .setting-info p,
  .input-setting p {
    margin: 0;
    font-size: 0.75rem;
    line-height: 1.4;
    color: var(--text-muted);
  }

  .input-setting {
    display: grid;
    grid-template-columns: 1fr 200px;
    column-gap: var(--space-xl);
    align-items: center;
    padding: var(--space-md) 0;
  }

  .input-setting h3 { grid-column: 1; grid-row: 1; }
  .input-setting p { grid-column: 1; grid-row: 2; }
  .input-setting .input { grid-column: 2; grid-row: 1 / span 2; }

  .plugin-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .plugin-item,
  .registry-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-md);
    padding: var(--space-sm) var(--space-md);
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-md);
  }

  .plugin-item--invalid {
    border-color: color-mix(in srgb, var(--accent-danger) 40%, transparent);
  }

  .plugin-info {
    flex: 1;
    min-width: 0;
  }

  .plugin-name {
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    margin: 0;
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--text-primary);
  }

  .plugin-link {
    font-size: 0.75rem;
    color: var(--accent-primary);
    text-decoration: none;
  }

  .plugin-link:hover {
    text-decoration: underline;
  }

  .plugin-path {
    margin: 0;
    font-family: var(--font-mono);
    font-size: 0.7rem;
    color: var(--text-muted);
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
  }

  .plugin-item--editing {
    flex-direction: column;
    align-items: stretch;
    gap: var(--space-sm);
    padding: var(--space-md);
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
    font-size: 0.75rem;
    font-weight: 500;
    min-width: 3ch;
    text-align: center;
  }

  .test-result.test-ok { color: var(--accent-success); }
  .test-result.test-fail { color: var(--accent-danger); }

  .plugin-command-input {
    font-family: var(--font-mono);
    font-size: 0.78rem;
  }

  .config-instructions {
    font-size: 0.78rem;
    color: var(--text-secondary);
    white-space: pre-line;
    margin-bottom: var(--space-lg);
    padding: var(--space-md) var(--space-lg);
    background: var(--bg-secondary);
    border-radius: var(--radius-md);
  }

  .regex-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .regex-item {
    display: flex;
    align-items: center;
    gap: var(--space-md);
  }

  .regex-number {
    font-family: var(--font-mono);
    font-size: 0.72rem;
    color: var(--text-muted);
    min-width: 22px;
  }

  .regex-input {
    flex: 1;
    font-family: var(--font-mono);
    font-size: 0.78rem;
  }

  .error-state {
    text-align: center;
    padding: var(--space-2xl);
  }

  .error-state h2 {
    margin-bottom: var(--space-lg);
    font-family: var(--font-serif);
    font-weight: 500;
    font-size: 1.05rem;
  }

  .registry-url-row {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: var(--space-sm);
    align-items: center;
  }

  .install-error,
  .registry-error,
  .config-error {
    color: var(--accent-danger);
    font-size: 0.75rem;
    margin: var(--space-xs) 0 var(--space-sm);
  }

  .registry-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .registry-author {
    margin: 0;
    font-size: 0.7rem;
    color: var(--text-muted);
  }

  .badge-version,
  .badge-installed,
  .badge-warning,
  .badge-healthy,
  .badge-coming-soon {
    display: inline-flex;
    align-items: center;
    height: 16px;
    padding: 0 6px;
    border-radius: var(--radius-sm);
    font-size: 0.66rem;
    font-weight: 500;
  }

  .badge-version {
    background: var(--bg-tertiary);
    color: var(--text-muted);
    font-family: var(--font-mono);
  }

  .badge-installed,
  .badge-healthy {
    background: color-mix(in srgb, var(--accent-success) 12%, transparent);
    color: var(--accent-success);
  }

  .badge-warning {
    background: color-mix(in srgb, var(--accent-danger) 12%, transparent);
    color: var(--accent-danger);
    cursor: help;
  }

  .badge-coming-soon {
    background: color-mix(in srgb, var(--accent-warning) 14%, transparent);
    color: var(--accent-warning);
  }

  .badge-update {
    height: 22px;
    padding: 0 var(--space-md);
    border-radius: var(--radius-md);
    border: 1px solid var(--accent-primary);
    background: var(--accent-soft);
    color: var(--accent-primary);
    font-size: 0.72rem;
    font-weight: 500;
    cursor: pointer;
    transition: background var(--transition-fast);
  }

  .badge-update:hover:not(:disabled) {
    background: var(--accent-ring);
  }

  .badge-update:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(20, 20, 19, 0.45);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
  }

  .modal {
    background: var(--bg-primary);
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-xl);
    width: 90%;
    max-width: 440px;
    max-height: 80vh;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-xl);
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    height: 42px;
    padding: 0 var(--space-sm) 0 var(--space-lg);
    border-bottom: 1px solid var(--border-primary);
  }

  .modal-title {
    margin: 0;
    font-family: var(--font-serif);
    font-size: 1rem;
    font-weight: 500;
    color: var(--text-primary);
  }

  .modal-body {
    flex: 1;
    overflow-y: auto;
    padding: var(--space-lg);
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-sm);
    padding: var(--space-md) var(--space-lg);
    border-top: 1px solid var(--border-primary);
  }

  .config-fields {
    display: flex;
    flex-direction: column;
    gap: var(--space-lg);
  }

  .config-field {
    display: flex;
    flex-direction: column;
    gap: var(--space-xs);
  }

  .config-label {
    font-size: 0.82rem;
    font-weight: 500;
    color: var(--text-primary);
  }

  .config-description {
    margin: 0 0 var(--space-xs);
    font-size: 0.72rem;
    color: var(--text-muted);
    line-height: 1.4;
  }

  .password-wrapper,
  .readonly-wrapper {
    position: relative;
    display: flex;
    align-items: center;
  }

  .password-wrapper .input,
  .readonly-wrapper .input {
    padding-right: 2.5rem;
  }

  .password-wrapper .copy-btn {
    right: 2rem;
  }

  .eye-toggle,
  .copy-btn {
    position: absolute;
    right: 0.4rem;
    background: none;
    border: none;
    cursor: pointer;
    font-size: 0.85rem;
    padding: 0.2rem;
    line-height: 1;
    opacity: 0.55;
    transition: opacity var(--transition-fast);
  }

  .eye-toggle:hover,
  .copy-btn:hover {
    opacity: 1;
  }

  input[readonly] {
    color: var(--text-muted);
    cursor: default;
  }

  .array-field {
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .array-item {
    display: flex;
    gap: var(--space-sm);
    align-items: center;
  }

  .array-item-input {
    flex: 1;
    min-width: 0;
  }

  .morph-rules {
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
  }

  .morph-rule {
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
    padding: var(--space-md);
    background: var(--bg-secondary);
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-md);
  }

  .morph-rule-top {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    gap: var(--space-md);
    align-items: center;
  }

  .morph-name {
    font-weight: 500;
  }

  .morph-kind {
    width: auto;
  }

  .morph-rule-fields {
    display: grid;
    grid-template-columns: 150px 1fr;
    gap: var(--space-sm) var(--space-md);
    align-items: center;
  }

  .morph-field-label {
    font-size: 0.72rem;
    color: var(--text-muted);
    line-height: 1.3;
  }

  .morph-mono {
    font-family: var(--font-mono);
    font-size: 0.78rem;
  }

  .morph-test {
    display: flex;
    flex-direction: column;
    gap: var(--space-sm);
    padding-top: var(--space-sm);
    border-top: 1px solid var(--border-primary);
  }

  .morph-test-toggle {
    align-self: flex-start;
    background: none;
    border: none;
    cursor: pointer;
    padding: 0;
    font-size: 0.75rem;
    color: var(--text-muted);
  }

  .morph-test-toggle:hover { color: var(--text-primary); }

  .morph-test-input {
    resize: vertical;
    min-height: 48px;
  }

  .morph-test-status {
    display: flex;
    gap: var(--space-sm);
    flex-wrap: wrap;
  }

  .morph-test-status.err {
    font-size: 0.75rem;
    color: var(--accent-danger);
  }

  .badge {
    display: inline-flex;
    align-items: center;
    height: 16px;
    font-size: 0.66rem;
    font-weight: 500;
    padding: 0 6px;
    border-radius: var(--radius-sm);
    background: var(--bg-tertiary);
  }

  .badge-ok {
    color: var(--accent-success);
    background: color-mix(in srgb, var(--accent-success) 12%, transparent);
  }

  .badge-muted { color: var(--text-muted); }

  .exec-check {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    font-size: 0.78rem;
    color: var(--text-secondary);
    cursor: pointer;
  }

  .exec-check input {
    accent-color: var(--accent-primary);
  }

  .morph-test-output {
    margin: 0;
    padding: var(--space-sm) var(--space-md);
    font-family: var(--font-mono);
    font-size: 0.75rem;
    line-height: 1.45;
    color: var(--text-primary);
    background: var(--bg-surface);
    border: 1px solid var(--border-primary);
    border-radius: var(--radius-sm);
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 160px;
    overflow-y: auto;
  }
</style>
