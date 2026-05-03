<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { isPermissionGranted, requestPermission, sendNotification } from '@tauri-apps/plugin-notification'
import { invoke } from '@tauri-apps/api/core'
import { useSessionStore } from './stores/session'
import { useSettingsStore } from './stores/settings'
import { useTheme } from './composables/useTheme'
import { usePanelResize } from './composables/usePanelResize'
import { useAttachments } from './composables/useAttachments'
import AtomStream from './components/AtomStream.vue'
import SessionPanel from './components/SessionPanel.vue'
import SettingsPanel from './components/SettingsPanel.vue'
import PragmaPanel from './components/PragmaPanel.vue'
import HelpPanel from './components/HelpPanel.vue'
import UsageModal from './components/UsageModal.vue'
import ComposerModal from './components/ComposerModal.vue'
import {
  TOAST_ERROR_DURATION_MS, TOAST_EXPORT_DURATION_MS, AUTO_CONFIRM_DELAY_MS,
  LS_FOCUS_MODE, LS_WORKING_DIR,
} from './PRAGMA_CONSTANTS'

const session = useSessionStore()
const settings = useSettingsStore()

const { isDark, applyTheme, toggleTheme } = useTheme()
const { leftWidth, rightWidth, startDrag } = usePanelResize()

const focusMode = ref(localStorage.getItem(LS_FOCUS_MODE) !== 'false')
watch(focusMode, v => localStorage.setItem(LS_FOCUS_MODE, String(v)))

// — Toasts —
const exportToast = ref<string | null>(null)
let exportToastTimer: ReturnType<typeof setTimeout> | null = null
const errorToast = ref<string | null>(null)
let errorToastTimer: ReturnType<typeof setTimeout> | null = null
const settingsSavedToast = ref(false)
let settingsSavedTimer: ReturnType<typeof setTimeout> | null = null

function showErrorToast(msg: string) {
  if (errorToastTimer) clearTimeout(errorToastTimer)
  errorToast.value = msg
  errorToastTimer = setTimeout(() => { errorToast.value = null }, TOAST_ERROR_DURATION_MS)
}

function showExportToast(path: string) {
  if (exportToastTimer) clearTimeout(exportToastTimer)
  exportToast.value = path
  exportToastTimer = setTimeout(() => { exportToast.value = null }, TOAST_EXPORT_DURATION_MS)
}

// — Attachments & working dir —
const { workingDir, attachments, attachmentExists, pickWorkingDir, attachFile, removeAttachment, handleDroppedPaths } = useAttachments(showErrorToast)

// — Modals —
const showSettings = ref(false)
const showUsage = ref(false)
const showHelp = ref(false)
const composerOpen = ref(false)
const confirmNewSession = ref(false)
const dragOver = ref(false)

// — Composer state (shared with ComposerModal via v-model) —
const prompt = ref('')
const selectedModel = ref<'sonnet' | 'opus'>('sonnet')
const autoConfirm = ref(true)

// — Profiles —
const profileIdx = ref(0)
watch(() => settings.loaded, (loaded) => {
  if (loaded) profileIdx.value = settings.settings.defaultProfileIndex ?? 0
}, { immediate: true })
const profiles = computed(() => settings.settings.profiles)
const noProfiles = computed(() => profiles.value.length === 0)

// — Export —
async function doExportJson() {
  const path = await session.exportCurrentSession()
  if (path) showExportToast(path)
}

async function doExportMd() {
  const path = await session.exportCurrentSessionMarkdown(workingDir.value || undefined)
  if (path) showExportToast(path)
}

function openUrl(url: string | null | undefined) {
  if (!url) return
  invoke('open_url', { url }).catch(console.error)
}

function onSettingsClose() {
  showSettings.value = false
  settings.load()
}

// — Composer open —
function openComposer() {
  if (session.running && session.pragmaPhase !== 'awaiting_answers') return
  composerOpen.value = true
}

// — New session confirm —
function tryNewSession() {
  if (attachments.value.length > 0) { confirmNewSession.value = true; return }
  session.newSession()
}

function doConfirmNewSession() {
  confirmNewSession.value = false
  attachments.value = []
  session.newSession()
}

// — Submit (called by ComposerModal @submit) —
async function submit(model: 'sonnet' | 'opus', ac: boolean) {
  const text = prompt.value.trim()
  if (!text) return

  if (session.pragmaPhase === 'awaiting_answers') {
    const p = profiles.value[profileIdx.value]
    if (!p) return
    await session.sendAnswers({ Q1: text }, p.binary, p.configDir ?? undefined, workingDir.value || undefined)
    prompt.value = ''
    composerOpen.value = false
    return
  }

  if (session.running || noProfiles.value) return
  const p = profiles.value[profileIdx.value]
  const filesToSave = [...attachments.value]
  session.sessionAttachments = []
  let fullPrompt = text
  if (filesToSave.length > 0) {
    fullPrompt += '\n\nAttached files in .pragmadocs/:\n' + filesToSave.map(f => `- ${f}`).join('\n')
  }
  try {
    await session.startSession(
      fullPrompt, p.binary, p.configDir ?? undefined,
      workingDir.value || undefined, p.label || undefined,
      session.sessionTitle || undefined, model,
      session.sessionId ?? undefined, filesToSave, !ac,
    )
  } catch (e: any) {
    session.lastError = String(e)
    session.sessionAttachments = filesToSave
    return
  }
  prompt.value = ''
  composerOpen.value = false
  selectedModel.value = 'sonnet'
  autoConfirm.value = false
}

// — Keyboard —
function handleGlobalKey(e: KeyboardEvent) {
  if (e.ctrlKey && e.key === 'k') { e.preventDefault(); openComposer() }
  if (e.ctrlKey && e.key === 's') { e.preventDefault(); showSettings.value = true }
}

// — Watches —
watch(() => session.pragmaPhase, (phase) => {
  if (phase === 'awaiting_answers') composerOpen.value = true
  if (phase === 'awaiting_confirmation' && autoConfirm.value) {
    setTimeout(() => {
      if (session.pragmaPhase === 'awaiting_confirmation' && autoConfirm.value) {
        const p = profiles.value[profileIdx.value]
        if (p) session.sendControl('CONTINUE', p.binary, p.configDir ?? undefined, workingDir.value || undefined)
      }
    }, AUTO_CONFIRM_DELAY_MS)
  }
})

watch(() => session.lastError, err => { if (err) showErrorToast(err) })

async function doSaveSession() {
  await session.saveCurrentSession()
  if (settingsSavedTimer) clearTimeout(settingsSavedTimer)
  settingsSavedToast.value = true
  settingsSavedTimer = setTimeout(() => { settingsSavedToast.value = false }, 2000)
}

watch(() => session.running, async (running, wasRunning) => {
  if (wasRunning && !running && settings.settings.notificationsEnabled) {
    let granted = await isPermissionGranted()
    if (!granted) granted = (await requestPermission()) === 'granted'
    if (!granted) return
    const title = session.sessionTitle || 'Session complete'
    const body = session.lastError ? `Error: ${session.lastError}` : 'Session finished.'
    sendNotification({ title: `Pragma — ${title}`, body })
  }
})

watch(
  () => [session.sessionTitle, session.running] as const,
  ([t, running]) => {
    const winTitle = t ? (running ? `▶ ${t} — Pragma` : `${t} — Pragma`) : 'Pragma'
    getCurrentWindow().setTitle(winTitle)
  },
  { immediate: true },
)

// — Mount —
onMounted(async () => {
  await settings.load()
  applyTheme(isDark.value)

  const lsDir = localStorage.getItem(LS_WORKING_DIR)
  if (lsDir !== null) {
    workingDir.value = lsDir
    settings.settings.lastWorkingDir = lsDir || null
    await settings.save()
    localStorage.removeItem(LS_WORKING_DIR)
  } else {
    workingDir.value = settings.settings.lastWorkingDir ?? ''
  }

  window.addEventListener('keydown', handleGlobalKey)
  await listen<{ paths: string[] }>('tauri://drag-drop', (e) => handleDroppedPaths(e.payload.paths))
  await listen('tauri://drag-enter', () => { dragOver.value = true })
  await listen('tauri://drag-leave', () => { dragOver.value = false })

  const splash = document.getElementById('pragma-splash')
  if (splash) {
    splash.classList.add('fade-out')
    setTimeout(() => splash.remove(), 260)
  }
})
</script>

<template>
  <div class="app-layout">
    <header class="header">
      <div class="header-top">
        <span class="logo">pragma</span>
        <button class="new-btn" title="New session" :disabled="session.running" @click="tryNewSession()">+</button>
        <input v-model="session.sessionTitle" class="title-input" placeholder="session title…" :disabled="session.running" />
        <Transition name="settings-saved">
          <span v-if="settingsSavedToast" class="settings-saved-chip">✓ saved</span>
        </Transition>
        <div class="header-actions">
          <button v-if="session.sessionId && !session.running && !session.reviewMode" class="action-btn" title="Save session" @click="doSaveSession()">save</button>
          <button v-if="session.sessionId && !session.running" class="action-btn export-btn" title="Export JSON to ~/.pragma/exports/" @click="doExportJson()">↓ JSON</button>
          <button v-if="session.sessionId && !session.running" class="action-btn export-btn" title="Export Markdown to ~/.pragma/exports/" @click="doExportMd()">↓ MD</button>
          <button class="action-btn" :class="{ 'focus-active': focusMode }" :title="focusMode ? 'Show all atoms' : 'Show signal only'" @click="focusMode = !focusMode">{{ focusMode ? 'focus' : 'raw' }}</button>
          <button class="action-btn" title="Usage stats" @click="showUsage = true">usage</button>
          <button class="action-btn" title="Guide" @click="showHelp = true">?</button>
          <button class="settings-btn" title="Settings" @click="showSettings = true">⚙</button>
        </div>
      </div>
    </header>

    <div class="main-layout">
      <SessionPanel :width="leftWidth" />
      <div class="resize-handle" @mousedown="startDrag('left', $event)" />
      <AtomStream :focus-mode="focusMode" :working-dir="workingDir || undefined" :attachments="attachments.length ? attachments : undefined" @open-composer="openComposer" />
      <div class="resize-handle" @mousedown="startDrag('right', $event)" />
      <PragmaPanel
        :profile="profiles[profileIdx] ?? null"
        :working-dir="workingDir || undefined"
        :width="rightWidth"
        @continue-session="composerOpen = true"
        @new-session="session.newSession()"
      />
    </div>

    <div class="bottom-bar">
      <div v-if="session.lastError" class="error-banner">
        <span class="error-text">
          <template v-if="session.lastError.includes('cannot start')">
            ⚠ Claude CLI not found — check the path in
            <button class="error-settings-link" @click="showSettings = true; session.lastError = null">⚙ Settings</button>
          </template>
          <template v-else>⚠ {{ session.lastError }}</template>
        </span>
        <button class="error-close" @click="session.lastError = null">×</button>
      </div>

      <div class="prompt-row">
        <select v-if="profiles.length > 1" v-model="profileIdx" class="binary-select" :disabled="session.running">
          <option v-for="(p, i) in profiles" :key="i" :value="i">{{ p.label }}</option>
        </select>
        <span v-else-if="profiles.length === 1" class="profile-label">{{ profiles[0].label }}</span>
        <button
          class="prompt-trigger"
          :class="{ 'has-text': prompt.length > 0, 'is-waiting': session.pragmaPhase === 'awaiting_answers' }"
          :disabled="(session.running && session.pragmaPhase !== 'awaiting_answers') || noProfiles"
          @click="openComposer"
        >{{ prompt || (noProfiles ? 'configure a profile in Settings…' : session.pragmaPhase === 'awaiting_answers' ? 'write your answer…' : 'prompt…') }}</button>
        <button
          class="run-btn"
          :disabled="(session.running && session.pragmaPhase !== 'awaiting_answers') || noProfiles"
          @click="prompt.trim() ? submit(selectedModel, autoConfirm) : openComposer()"
        >{{ session.pragmaPhase === 'awaiting_answers' ? 'awaiting input…' : session.running ? 'running…' : 'run' }}</button>
        <button v-if="session.running && session.pragmaPhase !== 'awaiting_answers'" class="stop-btn" title="Stop session" @click="session.stopSession()">stop</button>
      </div>

      <div class="header-workdir">
        <div class="workdir-group" :class="{ 'workdir-group--set': !!workingDir, 'workdir-group--disabled': session.running }" @click="!session.running && pickWorkingDir()">
          <span class="group-label">dir</span>
          <span class="workdir-icon">⌂</span>
          <span class="workdir-path" :class="{ empty: !workingDir }" :title="workingDir || ''">{{ workingDir || 'none' }}</span>
          <button v-if="workingDir" class="workdir-clear" :disabled="session.running" title="Remove directory" @click.stop="workingDir = ''">×</button>
        </div>
        <div
          class="attach-group"
          :class="{ 'attach-group--active': attachments.length > 0, 'attach-group--disabled': session.running }"
          :title="workingDir ? 'Attach file' : 'Choose working directory and attach file'"
          @click="!session.running && attachFile()"
        >
          <span class="group-label">files</span>
          <span class="attach-icon">📎</span>
          <span
            v-for="name in attachments"
            :key="name"
            class="attach-chip"
            :class="{ 'attach-chip--missing': attachmentExists[name] === false }"
            :title="attachmentExists[name] === false ? `File not found: ${name}` : name"
          >
            <span class="chip-name">{{ name }}</span>
            <button class="chip-remove" @click.stop="removeAttachment(name)">×</button>
          </span>
          <span v-if="!attachments.length" class="attach-empty">no files</span>
        </div>
      </div>
    </div>

    <footer class="app-footer">
      <div class="footer-left">
        <span>© 2026</span>
        <span class="footer-sep">—</span>
        <a class="footer-link" @click.prevent="openUrl('https://noema.tools')">noema.tools</a>
        <span class="footer-sep">—</span>
        <span>lightweb.it - P.IVA IT 02611230349</span>
      </div>
      <div class="footer-right">
        <a class="footer-link" @click.prevent="openUrl('https://noema.tools/pragma/terms')">Terms</a>
        <a class="footer-link" @click.prevent="openUrl('https://noema.tools/pragma/privacy')">Privacy</a>
        <a class="footer-link" @click.prevent="openUrl('https://noema.tools/pragma/license')">License</a>
        <button class="theme-btn" :title="isDark ? 'Switch to light theme' : 'Switch to dark theme'" @click="toggleTheme">{{ isDark ? '☀' : '🌙' }}</button>
      </div>
    </footer>

    <SettingsPanel v-if="showSettings" @close="onSettingsClose" />
    <HelpPanel v-if="showHelp" @close="showHelp = false" />

    <UsageModal :open="showUsage" :profile="profiles[profileIdx] ?? null" @close="showUsage = false" />

    <ComposerModal
      :open="composerOpen"
      :working-dir="workingDir"
      v-model:prompt="prompt"
      v-model:selected-model="selectedModel"
      v-model:auto-confirm="autoConfirm"
      @close="composerOpen = false"
      @submit="submit"
    />

    <Transition name="drag-overlay">
      <div v-if="dragOver" class="drag-overlay">
        <div class="drag-overlay-inner">
          <span class="drag-icon">📎</span>
          <span v-if="workingDir">Drop to attach</span>
          <span v-else class="drag-no-dir">Set a working directory before attaching files</span>
        </div>
      </div>
    </Transition>

    <Transition name="export-toast">
      <div v-if="exportToast" class="export-toast">
        <span class="export-toast-label">saved to</span>
        <span class="export-toast-path">{{ exportToast }}</span>
        <button class="export-toast-close" @click="exportToast = null">×</button>
      </div>
    </Transition>

    <Transition name="export-toast">
      <div v-if="errorToast" class="export-toast error-toast">
        <span class="export-toast-label">⚠</span>
        <span class="export-toast-path">{{ errorToast }}</span>
        <button class="export-toast-close" @click="errorToast = null">×</button>
      </div>
    </Transition>

    <Teleport to="body">
      <div v-if="confirmNewSession" class="confirm-overlay" @click.self="confirmNewSession = false">
        <div class="confirm-modal">
          <div class="confirm-modal-title">New session</div>
          <div class="confirm-modal-body">
            {{ attachments.length }} attached file{{ attachments.length !== 1 ? 's' : '' }} will be removed. Continue?
          </div>
          <div class="confirm-modal-actions">
            <button class="confirm-modal-cancel" @click="confirmNewSession = false">Cancel</button>
            <button class="confirm-modal-ok" @click="doConfirmNewSession">Continue</button>
          </div>
        </div>
      </div>
    </Teleport>
  </div>
</template>

<style scoped>
.app-layout {
  display: flex;
  flex-direction: column;
  height: 100vh;
}

.header {
  display: flex;
  flex-direction: column;
  background: var(--bg-secondary);
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}

.header-top {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 16px 4px;
}

.bottom-bar {
  display: flex;
  flex-direction: column;
  background: var(--bg-secondary);
  border-top: 1px solid var(--border-color);
  flex-shrink: 0;
}

.prompt-row {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  padding: 8px 16px;
}

.logo {
  font-size: 1.1rem;
  font-weight: 700;
  color: var(--color-atom-code);
  letter-spacing: 0.12em;
  flex-shrink: 0;
}

.new-btn {
  background: none;
  border: 1px solid var(--border-color);
  border-radius: 5px;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 1rem;
  font-weight: 300;
  padding: 0 7px;
  line-height: 1.6;
  flex-shrink: 0;
  transition: color 0.15s, border-color 0.15s;
}
.new-btn:hover:not(:disabled) { color: var(--text-primary); border-color: var(--text-secondary); }
.new-btn:disabled { opacity: 0.3; cursor: not-allowed; }

.title-input {
  flex: 1;
  background: transparent;
  border: none;
  border-bottom: 1px solid var(--border-color);
  border-radius: 0;
  padding: 2px 4px;
  color: var(--text-primary);
  font-family: inherit;
  font-size: 0.92rem;
  font-weight: 500;
  outline: none;
  transition: border-color 0.15s;
}
.title-input:focus { border-color: var(--color-atom-code); }
.title-input:disabled { opacity: 0.4; }
.title-input::placeholder { color: var(--text-secondary); opacity: 0.5; }

.settings-saved-chip {
  font-size: 0.72rem;
  font-weight: 600;
  color: #4caf50;
  white-space: nowrap;
  flex-shrink: 0;
}
.settings-saved-enter-active,
.settings-saved-leave-active { transition: opacity 0.2s ease; }
.settings-saved-enter-from,
.settings-saved-leave-to { opacity: 0; }

.header-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-shrink: 0;
}

.action-btn {
  padding: 4px 10px;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  color: var(--text-secondary);
  font-family: inherit;
  font-size: 0.78rem;
  cursor: pointer;
  transition: color 0.15s, border-color 0.15s;
}
.action-btn:hover { color: var(--text-primary); border-color: var(--text-secondary); }
.focus-active { color: var(--color-atom-code); border-color: var(--color-atom-code); background: rgba(106, 176, 245, 0.08); }
.export-btn { color: var(--color-atom-config); }

.settings-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 1rem;
  padding: 2px 4px;
  transition: color 0.15s;
}
.settings-btn:hover { color: var(--text-primary); }

.prompt-trigger {
  flex: 1;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 6px 12px;
  color: var(--text-secondary);
  font-family: inherit;
  font-size: 0.88rem;
  text-align: left;
  cursor: pointer;
  height: 32px;
  line-height: 20px;
  overflow: hidden;
  white-space: nowrap;
  text-overflow: ellipsis;
  transition: border-color 0.15s, color 0.15s;
}
.prompt-trigger.has-text { color: var(--text-primary); }
.prompt-trigger.is-waiting { border-color: #a07de0; color: #a07de0; }
.prompt-trigger:hover:not(:disabled) { border-color: var(--color-atom-code); }
.prompt-trigger:disabled { opacity: 0.5; cursor: not-allowed; }

.run-btn {
  padding: 6px 18px;
  background: var(--color-atom-code);
  color: #fff;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-family: inherit;
  font-size: 0.88rem;
  font-weight: 600;
  transition: background 0.15s;
  flex-shrink: 0;
}
.run-btn:hover:not(:disabled) { background: #5aa0e8; }
.run-btn:disabled { opacity: 0.4; cursor: not-allowed; }

.stop-btn {
  padding: 6px 14px;
  background: #c0392b;
  color: #fff;
  border: none;
  border-radius: 6px;
  cursor: pointer;
  font-family: inherit;
  font-size: 0.88rem;
  font-weight: 600;
  flex-shrink: 0;
  transition: background 0.15s;
}
.stop-btn:hover { background: #e74c3c; }

.error-banner {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 16px 4px;
  background: rgba(192, 57, 43, 0.15);
  border-top: 1px solid rgba(192, 57, 43, 0.4);
}
.error-text {
  flex: 1;
  font-size: 0.78rem;
  color: #e06c75;
  font-family: monospace;
  white-space: pre-wrap;
  word-break: break-all;
}
.error-close {
  background: none;
  border: none;
  color: #e06c75;
  cursor: pointer;
  font-size: 0.9rem;
  padding: 0 4px;
  flex-shrink: 0;
  line-height: 1;
}
.error-close:hover { color: #fff; }
.error-settings-link {
  background: none;
  border: none;
  color: #e06c75;
  cursor: pointer;
  font-size: inherit;
  font-family: inherit;
  padding: 0;
  text-decoration: underline;
  text-underline-offset: 2px;
}
.error-settings-link:hover { color: #fff; }

.binary-select {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 6px;
  padding: 6px 8px;
  color: var(--text-secondary);
  font-family: inherit;
  font-size: 0.82rem;
  outline: none;
  cursor: pointer;
  flex-shrink: 0;
}
.binary-select:disabled { opacity: 0.4; }

.profile-label {
  font-size: 0.82rem;
  color: var(--text-secondary);
  flex-shrink: 0;
}

.header-workdir {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 16px 8px;
}

.workdir-group,
.attach-group {
  display: flex;
  align-items: center;
  gap: 5px;
  padding: 3px 8px;
  border-radius: 6px;
  border: 1px solid transparent;
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s;
}
.workdir-group--disabled,
.attach-group--disabled { cursor: not-allowed; opacity: 0.5; }

.group-label {
  font-size: 0.62rem;
  font-weight: 600;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  opacity: 0.45;
  flex-shrink: 0;
  user-select: none;
}

.workdir-group {
  border-color: rgba(180, 130, 50, 0.25);
  background: rgba(180, 130, 50, 0.06);
}
.workdir-group:hover:not(.workdir-group--disabled) {
  border-color: rgba(180, 130, 50, 0.5);
  background: rgba(180, 130, 50, 0.12);
}
.workdir-group .group-label { color: #c8973a; opacity: 0.7; }
.workdir-group--set { border-color: rgba(180, 130, 50, 0.45); background: rgba(180, 130, 50, 0.1); }
.workdir-group--set .group-label { opacity: 1; }

.attach-group {
  border-color: rgba(80, 170, 160, 0.2);
  background: rgba(80, 170, 160, 0.05);
  flex-wrap: wrap;
  max-width: 600px;
}
.attach-group:hover:not(.attach-group--disabled) {
  border-color: rgba(80, 170, 160, 0.45);
  background: rgba(80, 170, 160, 0.1);
}
.attach-group .group-label { color: #50aaa0; opacity: 0.7; }
.attach-group--active { border-color: rgba(80, 170, 160, 0.45); background: rgba(80, 170, 160, 0.1); }
.attach-group--active .group-label { opacity: 1; }

.attach-empty { font-size: 0.72rem; font-style: italic; opacity: 0.35; }

.workdir-icon { color: #c8973a; font-size: 0.9rem; flex-shrink: 0; opacity: 0.8; pointer-events: none; }

.workdir-path {
  font-size: 0.75rem;
  font-family: monospace;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  direction: rtl;
  text-align: left;
  max-width: 420px;
  cursor: pointer;
  transition: color 0.15s;
}
.workdir-path:hover:not(.empty) { color: var(--text-primary); }
.workdir-path.empty { opacity: 0.4; cursor: default; font-family: inherit; font-style: italic; }

.workdir-clear {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 0.8rem;
  padding: 0 2px;
  flex-shrink: 0;
  line-height: 1;
}
.workdir-clear:hover { color: #e06c75; }

.attach-icon { flex-shrink: 0; pointer-events: none; font-size: 0.85rem; color: #50aaa0; opacity: 0.8; }

.attach-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  background: rgba(80, 170, 160, 0.12);
  border: 1px solid rgba(80, 170, 160, 0.35);
  border-radius: 12px;
  padding: 2px 8px 2px 10px;
  font-size: 0.72rem;
  font-family: monospace;
  color: #50aaa0;
  max-width: 200px;
}
.attach-chip--missing { background: rgba(200, 60, 60, 0.10); border-color: rgba(200, 60, 60, 0.40); color: #c44; }

.chip-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }

.chip-remove {
  background: none;
  border: none;
  color: #50aaa0;
  cursor: pointer;
  font-size: 0.75rem;
  padding: 0;
  line-height: 1;
  flex-shrink: 0;
  opacity: 0.7;
}
.chip-remove:hover { color: #e06c75; opacity: 1; }

.main-layout { display: flex; flex: 1; min-height: 0; overflow: hidden; }

.resize-handle {
  width: 4px;
  flex-shrink: 0;
  cursor: col-resize;
  background: transparent;
  position: relative;
  z-index: 10;
  transition: background 0.15s;
}
.resize-handle::after { content: ''; position: absolute; inset: 0; left: -3px; right: -3px; }
.resize-handle:hover { background: var(--color-atom-code); opacity: 0.45; }

.app-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 16px;
  background: var(--bg-secondary);
  border-top: 1px solid var(--border-color);
  flex-shrink: 0;
  font-size: 0.68rem;
  color: var(--text-secondary);
  gap: 8px;
}

.footer-left,
.footer-right { display: flex; align-items: center; gap: 6px; }
.footer-sep { opacity: 0.4; }

.footer-link {
  color: var(--color-atom-code);
  text-decoration: none;
  cursor: pointer;
  transition: opacity 0.15s;
}
.footer-link:hover { opacity: 0.75; }

.theme-btn {
  background: none;
  border: none;
  cursor: pointer;
  font-size: 0.85rem;
  padding: 0 2px;
  line-height: 1;
  transition: opacity 0.15s;
  color: inherit;
}
.theme-btn:hover { opacity: 0.7; }

.drag-overlay {
  position: fixed;
  inset: 0;
  z-index: 1000;
  background: rgba(40, 44, 52, 0.82);
  backdrop-filter: blur(3px);
  display: flex;
  align-items: center;
  justify-content: center;
  pointer-events: none;
}
.drag-overlay-inner {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 12px;
  padding: 40px 60px;
  border: 2px dashed var(--accent);
  border-radius: 16px;
  color: var(--text-primary);
  font-size: 1.1rem;
}
.drag-icon { font-size: 2.5rem; }
.drag-no-dir { color: #e06c75; font-size: 0.9rem; text-align: center; }
.drag-overlay-enter-active,
.drag-overlay-leave-active { transition: opacity 0.15s; }
.drag-overlay-enter-from,
.drag-overlay-leave-to { opacity: 0; }

.export-toast {
  position: fixed;
  bottom: 52px;
  right: 16px;
  z-index: 200;
  display: flex;
  align-items: center;
  gap: 8px;
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 7px;
  padding: 8px 12px;
  box-shadow: 0 4px 18px rgba(0,0,0,0.35);
  max-width: 480px;
}
.export-toast-label { font-size: 0.7rem; color: var(--text-secondary); white-space: nowrap; flex-shrink: 0; }
.export-toast-path { font-size: 0.72rem; font-family: monospace; color: var(--color-atom-config); word-break: break-all; flex: 1; }
.export-toast-close {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 1rem;
  line-height: 1;
  padding: 0 2px;
  flex-shrink: 0;
}
.export-toast-close:hover { color: var(--text-primary); }
.export-toast-enter-active,
.export-toast-leave-active { transition: opacity 0.18s ease, transform 0.18s ease; }
.export-toast-enter-from,
.export-toast-leave-to { opacity: 0; transform: translateY(8px); }

.error-toast {
  bottom: 96px;
  border-color: var(--error-color, #e05c5c);
  background: color-mix(in srgb, var(--error-color, #e05c5c) 12%, var(--bg-secondary));
}

.confirm-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0,0,0,0.45);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 9999;
  backdrop-filter: blur(2px);
}
.confirm-modal {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  padding: 28px 32px 24px;
  min-width: 280px;
  max-width: 360px;
  text-align: center;
  box-shadow: 0 8px 40px rgba(0,0,0,0.4);
}
.confirm-modal-title { font-size: 1rem; font-weight: 600; color: var(--text-primary); margin-bottom: 10px; }
.confirm-modal-body { font-size: 0.85rem; color: var(--text-secondary); margin-bottom: 22px; line-height: 1.5; }
.confirm-modal-actions { display: flex; gap: 10px; justify-content: center; }
.confirm-modal-cancel {
  padding: 7px 20px;
  border-radius: 7px;
  border: 1px solid var(--border-color);
  background: none;
  color: var(--text-primary);
  font-family: inherit;
  font-size: 0.85rem;
  cursor: pointer;
  transition: background 0.12s;
}
.confirm-modal-cancel:hover { background: var(--bg-secondary); }
.confirm-modal-ok {
  padding: 7px 20px;
  border-radius: 7px;
  border: none;
  background: var(--color-atom-code);
  color: #fff;
  font-family: inherit;
  font-size: 0.85rem;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.15s;
}
.confirm-modal-ok:hover { background: #5aa0e8; }
</style>
