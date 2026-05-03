<script setup lang="ts">
import { ref } from 'vue'
import type { PragmaTests } from '../stores/session'

const props = defineProps<{ data: PragmaTests; collapsed: boolean }>()
defineEmits<{ toggle: [] }>()

const checked = ref<boolean[]>([])

function ensureChecked() {
  while (checked.value.length < props.data.items.length) {
    checked.value.push(false)
  }
}

const copied = ref(false)
async function copyAll() {
  ensureChecked()
  const text = props.data.items
    .map((t, i) => `[${checked.value[i] ? 'x' : ' '}] ${t}`)
    .join('\n')
  await navigator.clipboard.writeText(text)
  copied.value = true
  setTimeout(() => { copied.value = false }, 1200)
}
</script>

<template>
  <div class="pragma-card tests-card">
    <div class="card-header" @click.self="$emit('toggle')">
      <button class="chevron-btn" :class="{ open: !collapsed }" @click="$emit('toggle')">
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none"><path d="M3 2L7 5L3 8" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>
      </button>
      <span class="card-label" @click="$emit('toggle')">TESTS</span>
      <span class="tests-count" @click="$emit('toggle')">
        {{ checked.filter(Boolean).length }}/{{ data.items.length }}
      </span>
      <button class="copy-btn" :class="{ copied }" :title="copied ? 'copied' : 'copy'" @click="copyAll">
        <svg width="13" height="13" viewBox="0 0 16 16" fill="none">
          <rect x="5" y="5" width="9" height="9" rx="1.5" stroke="currentColor" stroke-width="1.4"/>
          <path d="M3 11V3a1 1 0 0 1 1-1h8" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/>
        </svg>
      </button>
    </div>

    <div class="card-collapse" :class="{ 'is-collapsed': collapsed }">
      <div class="card-inner">
        <div class="tests-body">
          <label
            v-for="(item, i) in data.items"
            :key="i"
            class="test-item"
            :class="{ done: checked[i] }"
            @click.prevent="ensureChecked(); checked[i] = !checked[i]"
          >
            <span class="test-checkbox">
              <svg v-if="checked[i]" width="9" height="9" viewBox="0 0 9 9" fill="none">
                <path d="M1.5 4.5L3.5 6.5L7.5 2.5" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>
              </svg>
            </span>
            <span class="test-text">{{ item }}</span>
          </label>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.pragma-card {
  background: var(--bg-card);
  border: 1px solid var(--border-color);
  border-left: 3px solid #c98a30;
  border-radius: 6px;
  overflow: hidden;
}

.card-header {
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 6px 10px;
  background: rgba(201, 138, 48, 0.08);
  border-bottom: 1px solid var(--border-color);
  cursor: pointer;
  user-select: none;
  transition: background 0.12s;
}
.card-header:hover { background: rgba(201, 138, 48, 0.14); }

.chevron-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 0;
  line-height: 0;
  flex-shrink: 0;
  display: flex;
  align-items: center;
  transition: color 0.12s, transform 0.2s ease;
  transform: rotate(0deg);
}
.chevron-btn.open { transform: rotate(90deg); }
.chevron-btn:hover { color: var(--text-primary); }

.card-label {
  flex: 1;
  font-size: 0.68rem;
  font-weight: 700;
  letter-spacing: 0.1em;
  color: #c98a30;
  opacity: 0.9;
}

.tests-count {
  font-size: 0.65rem;
  color: var(--text-secondary);
  font-family: monospace;
}

.copy-btn {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  padding: 2px;
  line-height: 0;
  border-radius: 3px;
  transition: color 0.12s;
  display: flex;
  align-items: center;
}
.copy-btn:hover { color: var(--text-primary); }
.copy-btn.copied { color: #4caf50; }

.card-collapse {
  display: grid;
  grid-template-rows: 1fr;
  transition: grid-template-rows 0.24s ease;
  overflow: hidden;
}
.card-collapse.is-collapsed {
  grid-template-rows: 0fr;
}
.card-collapse > .card-inner {
  overflow: hidden;
  min-height: 0;
}

.tests-body {
  padding: 6px 10px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.test-item {
  display: flex;
  align-items: flex-start;
  gap: 8px;
  cursor: pointer;
  padding: 3px 2px;
  border-radius: 4px;
  transition: background 0.1s;
}
.test-item:hover { background: rgba(255,255,255,0.04); }

.test-checkbox {
  flex-shrink: 0;
  margin-top: 2px;
  width: 14px;
  height: 14px;
  border-radius: 3px;
  border: 1.5px solid rgba(150, 150, 190, 0.45);
  background: transparent;
  display: flex;
  align-items: center;
  justify-content: center;
  color: transparent;
  transition: border-color 0.15s, background 0.15s, color 0.15s;
}
.test-item.done .test-checkbox {
  border-color: rgba(76, 175, 80, 0.55);
  background: rgba(76, 175, 80, 0.07);
  color: #4caf50;
}

.test-text {
  font-size: 0.8rem;
  color: var(--text-primary);
  line-height: 1.4;
  transition: opacity 0.12s;
}
.test-item.done .test-text {
  opacity: 0.45;
  text-decoration: line-through;
}
</style>
