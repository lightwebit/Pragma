<script setup lang="ts">
import { ref, computed, onMounted, watch } from 'vue'
import { storeToRefs } from 'pinia'
import { useSessionStore, type SessionInfo } from '../stores/session'

const props = defineProps<{ width?: number }>()
const store = useSessionStore()
const { savedSessions, sessionId, running, sessionModel } = storeToRefs(store)
const collapsed = ref(false)
const searchQuery = ref('')
const searchResults = ref<SessionInfo[]>([])
let searchTimer: ReturnType<typeof setTimeout> | null = null

const confirmTarget = ref<SessionInfo | null>(null)

function askDelete(e: MouseEvent, s: SessionInfo) {
  e.stopPropagation()
  confirmTarget.value = s
}

async function confirmDelete() {
  if (!confirmTarget.value) return
  await store.deleteSession(confirmTarget.value.id)
  confirmTarget.value = null
}

const displayedSessions = computed<SessionInfo[]>(() => {
  const base = searchQuery.value.trim() ? searchResults.value : savedSessions.value
  if (sessionId.value && !base.find(s => s.id === sessionId.value)) {
    return [{ id: sessionId.value, title: null, startedAt: '', command: null, workingDir: null, atomCount: 0, inputTokens: 0, outputTokens: 0, cacheReadTokens: 0, cacheWriteTokens: 0, totalCostUsd: null, attachments: [] }, ...base]
  }
  return base
})

onMounted(() => {
  store.fetchSavedSessions()
})

watch(searchQuery, (q) => {
  if (searchTimer) clearTimeout(searchTimer)
  if (!q.trim()) return
  searchTimer = setTimeout(async () => {
    searchResults.value = await store.searchSessions(q)
  }, 250)
})

function formatDate(iso: string): string {
  const d = new Date(iso)
  return d.toLocaleDateString('en-GB', {
    day: '2-digit',
    month: '2-digit',
    year: '2-digit',
  }) + ' ' + d.toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' })
}

function shortId(id: string): string {
  return id.slice(0, 8)
}

async function openSession(s: SessionInfo) {
  if (running.value) return
  await store.loadSession(s.id)
}

</script>

<template>
  <aside class="session-panel" :class="{ 'is-collapsed': collapsed }" :style="!collapsed && props.width ? { width: props.width + 'px' } : undefined">
    <div class="panel-header">
      <template v-if="!collapsed">
        <span class="panel-title">Sessions</span>
      </template>
      <button
        class="collapse-btn"
        :title="collapsed ? 'Expand' : 'Collapse'"
        @click="collapsed = !collapsed"
      >{{ collapsed ? '›' : '‹' }}</button>
    </div>

    <template v-if="!collapsed">
      <div class="search-row">
        <input
          v-model="searchQuery"
          class="search-input"
          placeholder="search…"
          type="search"
        />
      </div>

      <div v-if="displayedSessions.length === 0" class="empty-msg">
        {{ searchQuery ? 'No results.' : 'No saved sessions.' }}
      </div>

      <ul v-else class="session-list">
        <li
          v-for="s in displayedSessions"
          :key="s.id"
          class="session-item"
          :class="{ active: sessionId === s.id, locked: running && sessionId !== s.id }"
          :title="`ID: ${s.id}\n${s.command ?? ''}`"
          @click="openSession(s)"
        >
          <div class="session-row">
            <span
              class="session-name"
              :class="{
                'session-running': s.id === sessionId && running,
                'opus-running': s.id === sessionId && running && sessionModel === 'opus',
              }"
            >
              {{ s.title || shortId(s.id) }}
              <span
                v-if="s.id === store.sessionId && store.running"
                class="running-dot"
                :class="{ 'opus-running': sessionModel === 'opus' }"
                title="running"
              >●</span>
            </span>
            <div class="session-row-right">
              <span class="session-atoms">{{ s.atomCount }}</span>
              <button
                class="delete-btn"
                title="Delete session"
                @click="askDelete($event, s)"
              >✕</button>
            </div>
          </div>
          <div class="session-date">{{ formatDate(s.startedAt) }}</div>
        </li>
      </ul>
    </template>
  </aside>

  <Teleport to="body">
    <div v-if="confirmTarget" class="delete-overlay" @click.self="confirmTarget = null">
      <div class="delete-modal">
        <div class="delete-modal-icon">🗑</div>
        <div class="delete-modal-title">Delete session?</div>
        <div class="delete-modal-name">{{ confirmTarget.title || confirmTarget.id.slice(0, 8) }}</div>
        <div class="delete-modal-hint">This action cannot be undone.</div>
        <div class="delete-modal-actions">
          <button class="delete-modal-cancel" @click="confirmTarget = null">Cancel</button>
          <button class="delete-modal-confirm" @click="confirmDelete">Delete</button>
        </div>
      </div>
    </div>
  </Teleport>

</template>

<style scoped>
.session-panel {
  width: 220px;
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  background: var(--bg-secondary);
  border-right: 1px solid var(--border-color);
  overflow: hidden;
  transition: none;
}

.session-panel.is-collapsed {
  width: 28px;
}

.panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 8px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
  gap: 4px;
  min-height: 37px;
}

.panel-title {
  font-size: 0.78rem;
  font-weight: 600;
  letter-spacing: 0.08em;
  color: var(--text-secondary);
  text-transform: uppercase;
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
}

.refresh-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 1rem;
  padding: 0 2px;
  line-height: 1;
  flex-shrink: 0;
  transition: color 0.15s;
}

.refresh-btn:hover { color: var(--text-primary); }

.collapse-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 1rem;
  padding: 0 2px;
  line-height: 1;
  flex-shrink: 0;
  transition: color 0.15s;
}

.collapse-btn:hover { color: var(--text-primary); }

.search-row {
  padding: 6px 8px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}

.search-input {
  width: 100%;
  box-sizing: border-box;
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 5px;
  padding: 4px 8px;
  color: var(--text-primary);
  font-family: inherit;
  font-size: 0.78rem;
  outline: none;
  transition: border-color 0.15s;
}
.search-input:focus { border-color: var(--color-atom-code); }
.search-input::placeholder { color: var(--text-secondary); opacity: 0.5; }

.empty-msg {
  padding: 16px 12px;
  font-size: 0.78rem;
  color: var(--text-secondary);
  font-style: italic;
}

.session-list {
  list-style: none;
  margin: 0;
  padding: 4px 0;
  overflow-y: auto;
  flex: 1;
}

.session-item {
  padding: 8px 12px;
  cursor: pointer;
  border-left: 3px solid transparent;
  transition: background 0.12s, border-color 0.12s;
}

.session-item:hover { background: var(--bg-card); }
.session-item.locked { cursor: not-allowed; opacity: 0.5; }

.session-item.active {
  border-left-color: var(--color-atom-code);
  background: var(--bg-card);
}

.session-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 2px;
}

.session-row-right {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}

.delete-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-size: 0.65rem;
  padding: 0 2px;
  line-height: 1;
  opacity: 0;
  transition: opacity 0.15s, color 0.15s;
}

.session-item:hover .delete-btn {
  opacity: 1;
}

.delete-btn:hover {
  color: #e05555;
}

.session-running {
  color: var(--color-atom-code);
  font-style: italic;
}
.session-running.opus-running { color: #a07de0; }

.running-dot {
  font-style: normal;
  font-size: 0.6rem;
  color: var(--color-atom-code);
  margin-left: 3px;
  animation: blink 1.2s step-start infinite;
}
.running-dot.opus-running { color: #a07de0; }

@keyframes blink {
  50% { opacity: 0; }
}

.session-name {
  font-size: 0.82rem;
  color: var(--text-primary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 155px;
}

.session-atoms {
  font-size: 0.72rem;
  color: var(--text-secondary);
  background: var(--bg-secondary);
  border-radius: 10px;
  padding: 1px 6px;
  flex-shrink: 0;
}

.session-date {
  font-size: 0.72rem;
  color: var(--text-secondary);
}
</style>

<style>
.delete-overlay {
  position: fixed;
  inset: 0;
  z-index: 9000;
  background: rgba(0,0,0,0.45);
  display: flex;
  align-items: center;
  justify-content: center;
  backdrop-filter: blur(2px);
}

.delete-modal {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-radius: 12px;
  padding: 28px 32px 24px;
  min-width: 280px;
  max-width: 360px;
  text-align: center;
  box-shadow: 0 8px 40px rgba(0,0,0,0.4);
}

.delete-modal-icon {
  font-size: 2rem;
  margin-bottom: 12px;
}

.delete-modal-title {
  font-size: 1rem;
  font-weight: 600;
  color: var(--text-primary);
  margin-bottom: 6px;
}

.delete-modal-name {
  font-size: 0.82rem;
  color: var(--color-atom-code);
  font-family: monospace;
  background: var(--bg-secondary);
  border-radius: 6px;
  padding: 4px 10px;
  display: inline-block;
  margin-bottom: 10px;
}

.delete-modal-hint {
  font-size: 0.78rem;
  color: var(--text-secondary);
  margin-bottom: 20px;
}

.delete-modal-actions {
  display: flex;
  gap: 10px;
  justify-content: center;
}

.delete-modal-cancel {
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
.delete-modal-cancel:hover { background: var(--bg-secondary); }

.delete-modal-confirm {
  padding: 7px 20px;
  border-radius: 7px;
  border: none;
  background: #c0392b;
  color: #fff;
  font-family: inherit;
  font-size: 0.85rem;
  font-weight: 600;
  cursor: pointer;
  transition: background 0.12s;
}
.delete-modal-confirm:hover { background: #e74c3c; }
</style>
