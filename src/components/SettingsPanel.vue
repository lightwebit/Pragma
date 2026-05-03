<script setup lang="ts">
import { ref, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useSettingsStore, type Profile } from '../stores/settings'

const emit = defineEmits<{ close: [] }>()
const store = useSettingsStore()

const saving = ref(false)
const detecting = ref(false)

// Local copy for editing (does not modify the store until saved)
const profiles = ref<Profile[]>(
  store.settings.profiles.map(p => ({ ...p }))
)
const notificationsEnabled = ref(store.settings.notificationsEnabled ?? false)
const diffAlwaysOpen = ref(store.settings.diffAlwaysOpen ?? false)
const defaultIdx = ref(store.settings.defaultProfileIndex ?? 0)
const exportDir = ref(store.settings.exportDir ?? '')

function addProfile() {
  profiles.value.push({ label: '', binary: '', configDir: null, language: 'English' })
}

function removeProfile(i: number) {
  profiles.value.splice(i, 1)
}

async function autodetect(i: number) {
  detecting.value = true
  const path = await store.detectClaude()
  if (path) profiles.value[i].binary = path
  detecting.value = false
}

async function browseExportDir() {
  const picked = await invoke<string | null>('pick_directory')
  if (picked) exportDir.value = picked
}

const saveError = ref<string | null>(null)

const trustedDirs = ref<string[]>([])

function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'Escape') emit('close')
}

onMounted(async () => {
  trustedDirs.value = await invoke<string[]>('get_trusted_dirs')
  window.addEventListener('keydown', onKeyDown)
})

onUnmounted(() => {
  window.removeEventListener('keydown', onKeyDown)
})

async function removeTrustedDir(dir: string) {
  await invoke('remove_trusted_dir', { dir })
  trustedDirs.value = trustedDirs.value.filter(d => d !== dir)
}

async function saveAndClose() {
  saving.value = true
  saveError.value = null
  store.settings.profiles = profiles.value.map(p => ({ ...p }))
  store.settings.notificationsEnabled = notificationsEnabled.value
  store.settings.diffAlwaysOpen = diffAlwaysOpen.value
  store.settings.defaultProfileIndex = defaultIdx.value
  store.settings.exportDir = exportDir.value.trim() || null
  try {
    await store.save()
    saving.value = false
    emit('close')
  } catch (e) {
    saving.value = false
    saveError.value = `Save failed: ${String(e)}`
    console.error('[pragma] settings save:', e)
  }
}
</script>

<template>
  <div class="overlay" @click.self="emit('close')">
    <div class="panel">
      <div class="panel-header">
        <span>Settings</span>
        <button class="close-btn" @click="emit('close')">✕</button>
      </div>

      <div class="panel-body">
      <div class="section">
        <div class="section-title">Claude Profiles</div>

        <div
          v-for="(p, i) in profiles"
          :key="i"
          class="profile-card"
        >
          <!-- Main row -->
          <div class="profile-main-row">
            <button
              class="default-btn"
              :class="{ 'is-default': defaultIdx === i }"
              :title="defaultIdx === i ? 'Default profile' : 'Set as default'"
              @click="defaultIdx = i"
            >{{ defaultIdx === i ? '★' : '☆' }}</button>
            <input
              v-model="p.label"
              class="input label-input"
              placeholder="label"
            />
            <input
              v-model="p.binary"
              class="input binary-input"
              placeholder="claude binary path"
            />
            <button
              class="detect-btn"
              :disabled="detecting"
              title="Auto-detect"
              @click="autodetect(i)"
            >⌕</button>
            <button class="remove-btn" title="Remove" @click="removeProfile(i)">✕</button>
          </div>

          <!-- Config dir -->
          <div class="profile-sub-row">
            <input
              v-model="p.configDir"
              class="input config-input"
              placeholder="CLAUDE_CONFIG_DIR (optional)"
            />
          </div>

          <!-- Language -->
          <div class="profile-sub-row">
            <input
              v-model="p.language"
              class="input lang-input"
              placeholder="Language (e.g. English, Italiano, Français…)"
            />
          </div>

          <div class="profile-sub-row">
            <span class="prompt-hint">System prompt: <code>~/.pragma/prompts/{{ p.label.toLowerCase().replace(/\s+/g, '_') || 'label' }}.txt</code></span>
          </div>
        </div>

        <button class="add-btn" @click="addProfile">+ add profile</button>
      </div>

      <div class="section notif-section">
        <div class="section-title">Notifications</div>
        <label class="toggle-row">
          <input type="checkbox" v-model="notificationsEnabled" class="toggle-checkbox" />
          <span class="toggle-label">Notify when session ends</span>
        </label>
      </div>

      <div class="section">
        <div class="section-title">Stream</div>
        <label class="toggle-row">
          <input type="checkbox" v-model="diffAlwaysOpen" class="toggle-checkbox" />
          <span class="toggle-label">Always expand diff atoms</span>
        </label>
      </div>

      <div class="section export-section">
        <div class="section-title">Export Directory</div>
        <div class="export-row">
          <input
            v-model="exportDir"
            class="input export-input"
            placeholder="Leave empty to use ~/Downloads"
          />
          <button class="browse-btn" title="Browse" @click="browseExportDir">…</button>
        </div>
        <div class="export-hint">Default: <code>~/Downloads</code> (OS-specific)</div>
      </div>

      <div class="section">
        <div class="section-title">Trusted Directories</div>
        <div class="export-hint" style="margin-bottom:8px">Directories auto-trusted on first use. Remove to forget.</div>
        <div v-if="trustedDirs.length === 0" class="export-hint" style="color:var(--text-muted)">None yet.</div>
        <div v-for="dir in trustedDirs" :key="dir" class="trusted-dir-row">
          <span class="trusted-dir-path">{{ dir }}</span>
          <button class="remove-trust-btn" title="Remove" @click="removeTrustedDir(dir)">✕</button>
        </div>
      </div>
      </div><!-- /panel-body -->

      <div v-if="saveError" class="save-error">⚠ {{ saveError }}</div>
      <div class="footer">
        <button class="cancel-btn" @click="emit('close')">Cancel</button>
        <button
          class="save-btn"
          :disabled="saving"
          @click="saveAndClose"
        >
          {{ saving ? '…' : 'Save' }}
        </button>
      </div>
    </div>
  </div>

</template>

<style scoped>
.overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 100;
}

.panel {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  width: 640px;
  max-width: 95vw;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 14px 18px;
  border-bottom: 1px solid var(--border-color);
  font-weight: 600;
  font-size: 0.95rem;
}

.close-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 0.9rem;
  padding: 2px 4px;
}
.close-btn:hover { color: var(--text-primary); }

.panel-body {
  flex: 1;
  overflow-y: auto;
}

.section {
  padding: 16px 18px;
}

.section-title {
  font-size: 0.78rem;
  font-weight: 600;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  color: var(--text-secondary);
  margin-bottom: 12px;
}

.profile-card {
  border: 1px solid var(--border-color);
  border-radius: 7px;
  padding: 10px 12px;
  margin-bottom: 10px;
  background: var(--bg-card);
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.profile-main-row {
  display: flex;
  gap: 6px;
  align-items: center;
}

.profile-sub-row {
  display: flex;
  gap: 6px;
}

.prompt-toggle-row {
  margin-top: 2px;
}

.prompt-area-row {
  display: flex;
}

.input {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 5px 8px;
  color: var(--text-primary);
  font-family: inherit;
  font-size: 0.82rem;
  outline: none;
}
.input:focus { border-color: var(--color-atom-code); }

.label-input  { width: 110px; flex-shrink: 0; }
.binary-input { flex: 1; min-width: 0; font-family: monospace; font-size: 0.78rem; }
.config-input { flex: 1; min-width: 0; font-family: monospace; font-size: 0.78rem; }
.lang-input   { flex: 1; min-width: 0; }

.prompt-hint {
  font-size: 0.72rem;
  color: var(--text-secondary);
  opacity: 0.65;
}
.prompt-hint code {
  font-family: monospace;
  font-size: 0.72rem;
  opacity: 0.9;
}

.detect-btn {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 5px 8px;
  font-size: 0.9rem;
  flex-shrink: 0;
}
.detect-btn:hover:not(:disabled) { color: var(--text-primary); }
.detect-btn:disabled { opacity: 0.4; }

.default-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 0.95rem;
  padding: 2px 4px;
  flex-shrink: 0;
  line-height: 1;
  transition: color 0.12s;
}
.default-btn:hover { color: #e5a338; }
.default-btn.is-default { color: #e5a338; }

.remove-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 4px 6px;
  font-size: 0.82rem;
  flex-shrink: 0;
}
.remove-btn:hover { color: #e06c75; }

.add-btn {
  background: none;
  border: 1px dashed var(--border-color);
  border-radius: 6px;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 5px 12px;
  font-size: 0.80rem;
  width: 100%;
  margin-top: 4px;
  transition: border-color 0.15s, color 0.15s;
}
.add-btn:hover {
  border-color: var(--color-atom-code);
  color: var(--color-atom-code);
}

.url-row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 8px;
}
.url-label {
  font-size: 0.78rem;
  color: var(--text-secondary);
  width: 80px;
  flex-shrink: 0;
}
.url-input {
  flex: 1;
  font-family: monospace;
  font-size: 0.78rem;
}

.notif-section {
  border-top: 1px solid var(--border-color);
  padding-top: 14px;
}

.export-section {
  border-top: 1px solid var(--border-color);
  padding-top: 14px;
}

.export-row {
  display: flex;
  gap: 6px;
  align-items: center;
}

.export-input {
  flex: 1;
  min-width: 0;
  font-family: monospace;
  font-size: 0.78rem;
}

.browse-btn {
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 5px 10px;
  font-size: 0.85rem;
  flex-shrink: 0;
}
.browse-btn:hover { color: var(--text-primary); }

.export-hint {
  margin-top: 5px;
  font-size: 0.72rem;
  color: var(--text-secondary);
  opacity: 0.65;
}
.export-hint code {
  font-family: monospace;
  font-size: 0.72rem;
}

.toggle-row {
  display: flex;
  align-items: center;
  gap: 10px;
  cursor: pointer;
}

.toggle-checkbox {
  width: 15px;
  height: 15px;
  cursor: pointer;
  accent-color: var(--color-atom-code);
}

.toggle-label {
  font-size: 0.85rem;
  color: var(--text-primary);
}

.footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 18px;
  border-top: 1px solid var(--border-color);
}

.cancel-btn {
  background: none;
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: var(--text-secondary);
  padding: 6px 16px;
  cursor: pointer;
  font-family: inherit;
  font-size: 0.85rem;
}

.save-btn {
  background: var(--color-atom-code);
  border: none;
  border-radius: 6px;
  color: #fff;
  padding: 6px 20px;
  cursor: pointer;
  font-family: inherit;
  font-size: 0.85rem;
  font-weight: 600;
  min-width: 70px;
}
.save-btn:disabled { opacity: 0.6; }

.save-error {
  padding: 6px 12px;
  color: var(--error-color, #e05c5c);
  font-size: 0.8rem;
  border-top: 1px solid var(--border-color);
}

.trusted-dir-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 0;
  border-bottom: 1px solid var(--border-color);
}
.trusted-dir-path {
  flex: 1;
  font-size: 0.78rem;
  color: var(--text-secondary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.remove-trust-btn {
  background: none;
  border: none;
  color: var(--text-muted);
  cursor: pointer;
  font-size: 0.75rem;
  padding: 2px 4px;
  line-height: 1;
}
.remove-trust-btn:hover { color: var(--error-color, #e05c5c); }
</style>

