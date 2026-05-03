<script setup lang="ts">
import { ref, watch, nextTick } from 'vue'
import { useSessionStore } from '../stores/session'

const props = defineProps<{
  open: boolean
  workingDir: string
}>()

const emit = defineEmits<{
  close: []
  submit: [model: 'sonnet' | 'opus', autoConfirm: boolean]
}>()

const prompt = defineModel<string>('prompt', { default: '' })
const selectedModel = defineModel<'sonnet' | 'opus'>('selectedModel', { default: 'sonnet' })
const autoConfirm = defineModel<boolean>('autoConfirm', { default: true })

const session = useSessionStore()
const advancedOpen = ref(false)
const textareaRef = ref<HTMLTextAreaElement | null>(null)

watch(() => props.open, async (open) => {
  if (open) {
    await nextTick()
    textareaRef.value?.focus()
  } else {
    advancedOpen.value = false
  }
})

function submit() {
  emit('submit', selectedModel.value, autoConfirm.value)
}
</script>

<template>
  <Teleport to="body">
    <Transition name="composer-fade">
      <div v-if="open" class="composer-backdrop" @click.self="emit('close')">
        <div class="composer-modal">
          <div v-if="session.pragmaPhase === 'awaiting_answers' || session.sessionTitle"
               class="composer-banner"
               :class="session.pragmaPhase === 'awaiting_answers' ? 'banner-answering' : 'banner-continue'">
            <span class="banner-icon">{{ session.pragmaPhase === 'awaiting_answers' ? '?' : '↩' }}</span>
            <div class="banner-body">
              <span class="banner-title">{{ session.pragmaPhase === 'awaiting_answers' ? 'Waiting for your answers' : session.sessionTitle }}</span>
              <span v-if="session.pragmaPhase === 'awaiting_answers'" class="banner-sub">Respond below to continue the session</span>
              <span v-else-if="workingDir" class="banner-sub">{{ workingDir }}</span>
            </div>
          </div>
          <div v-if="session.pragmaQuestions && session.pragmaPhase === 'awaiting_answers'" class="composer-context">
            <div class="context-label">Questions:</div>
            <ol class="context-questions">
              <li v-for="(q, i) in session.pragmaQuestions.items" :key="i">{{ q }}</li>
            </ol>
          </div>
          <textarea
            ref="textareaRef"
            v-model="prompt"
            class="composer-textarea"
            :placeholder="session.pragmaPhase === 'awaiting_answers' ? 'Answer the questions…' : 'prompt…'"
            @keydown.enter.ctrl.prevent="submit"
            @keydown.enter.meta.prevent="submit"
            @keydown.escape.prevent="emit('close')"
          />
          <div v-if="session.pragmaPhase !== 'awaiting_answers'" class="composer-advanced">
            <button class="advanced-toggle" @click="advancedOpen = !advancedOpen">
              Advanced settings {{ advancedOpen ? '▴' : '▾' }}
            </button>
            <div v-if="advancedOpen" class="advanced-body">
              <div class="adv-group">
                <span class="adv-label">Model</span>
                <label class="adv-radio">
                  <input type="radio" v-model="selectedModel" value="sonnet" />
                  Sonnet <span class="adv-hint">(default)</span>
                </label>
                <label class="adv-radio">
                  <input type="radio" v-model="selectedModel" value="opus" />
                  Opus <span class="adv-hint">— complex tasks: architecture, refactoring</span>
                </label>
              </div>
              <div class="adv-group">
                <label class="adv-radio">
                  <input type="checkbox" v-model="autoConfirm" />
                  Auto-approve steps <span class="adv-hint">— uncheck to confirm each step manually</span>
                </label>
              </div>
            </div>
          </div>
          <div class="composer-footer">
            <span class="composer-hint">
              <kbd>Ctrl+Enter</kbd> send · <kbd>Esc</kbd> close
              <span class="hint-sep">|</span>
              <span class="hint-global"><kbd>Ctrl+K</kbd> open · <kbd>Ctrl+S</kbd> settings</span>
            </span>
            <div class="composer-footer-right">
              <span v-if="selectedModel === 'opus'" class="model-badge">opus</span>
              <span v-if="autoConfirm" class="model-badge auto-badge">auto-continue</span>
              <button class="run-btn" :disabled="!prompt.trim()" @click="submit">
                {{ session.pragmaPhase === 'awaiting_answers' ? 'Send answer' : 'Run' }}
              </button>
            </div>
          </div>
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.composer-backdrop {
  position: fixed;
  inset: 0;
  z-index: 500;
  background: rgba(20, 22, 28, 0.65);
  display: flex;
  align-items: center;
  justify-content: center;
}

.composer-modal {
  width: min(680px, 92vw);
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 10px;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  box-shadow: 0 16px 40px rgba(0,0,0,0.5);
}

.composer-banner {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 16px;
  border-bottom: 1px solid var(--border-color);
}

.banner-answering { background: rgba(212, 168, 67, 0.08); }
.banner-continue  { background: rgba(160, 125, 224, 0.06); }

.banner-icon {
  font-size: 0.95rem;
  line-height: 1;
  flex-shrink: 0;
  width: 18px;
  text-align: center;
}

.banner-answering .banner-icon { color: var(--color-atom-data); }
.banner-continue .banner-icon  { color: var(--color-atom-style); }

.banner-body {
  display: flex;
  flex-direction: column;
  gap: 1px;
  min-width: 0;
}

.banner-title {
  font-size: 0.80rem;
  font-weight: 600;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.banner-answering .banner-title { color: var(--color-atom-data); }
.banner-continue .banner-title  { color: var(--text-primary); }

.banner-sub {
  font-size: 0.68rem;
  color: var(--text-secondary);
  opacity: 0.7;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.composer-context {
  padding: 12px 16px 8px;
  border-bottom: 1px solid var(--border-color);
  background: rgba(160, 125, 224, 0.06);
}

.context-label {
  font-size: 0.68rem;
  font-weight: 700;
  letter-spacing: 0.1em;
  color: #a07de0;
  margin-bottom: 6px;
  text-transform: uppercase;
}

.context-questions {
  margin: 0;
  padding-left: 18px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.context-questions li {
  font-size: 0.82rem;
  color: var(--text-primary);
  line-height: 1.45;
}

.composer-textarea {
  flex: 1;
  background: transparent;
  border: none;
  outline: none;
  resize: none;
  padding: 16px;
  color: var(--text-primary);
  font-family: inherit;
  font-size: 0.92rem;
  line-height: 1.55;
  min-height: 180px;
  max-height: 60vh;
}

.composer-advanced {
  border-top: 1px solid var(--border-color);
}

.advanced-toggle {
  background: none;
  border: none;
  color: var(--text-secondary);
  cursor: pointer;
  font-family: inherit;
  font-size: 0.72rem;
  padding: 6px 16px;
  width: 100%;
  text-align: left;
  transition: color 0.12s;
}
.advanced-toggle:hover { color: var(--text-primary); }

.advanced-body {
  padding: 8px 16px 10px;
  display: flex;
  flex-direction: column;
  gap: 10px;
  background: var(--bg-card);
}

.adv-group {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.adv-label {
  font-size: 0.68rem;
  font-weight: 700;
  letter-spacing: 0.08em;
  color: var(--text-secondary);
  text-transform: uppercase;
  margin-bottom: 2px;
}

.adv-radio {
  display: flex;
  align-items: center;
  gap: 7px;
  font-size: 0.82rem;
  color: var(--text-primary);
  cursor: pointer;
}

.adv-hint {
  font-size: 0.74rem;
  color: var(--text-secondary);
  opacity: 0.75;
}

.composer-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 16px;
  border-top: 1px solid var(--border-color);
  background: var(--bg-card);
}

.composer-hint {
  font-size: 0.72rem;
  color: var(--text-secondary);
  opacity: 0.6;
  display: flex;
  align-items: center;
  gap: 5px;
  flex-wrap: wrap;
}

.composer-hint kbd {
  font-family: inherit;
  font-size: 0.70rem;
  font-weight: 700;
  color: var(--text-primary);
  background: var(--bg-secondary);
  border: 1px solid var(--border-color);
  border-radius: 3px;
  padding: 0px 4px;
}

.hint-sep { opacity: 0.4; margin: 0 2px; }

.hint-global { opacity: 0.75; }
.hint-global kbd {
  font-weight: 400;
  color: var(--text-secondary);
  background: transparent;
  border-color: var(--border-color);
  opacity: 0.8;
}

.composer-footer-right {
  display: flex;
  align-items: center;
  gap: 8px;
}

.model-badge {
  font-size: 0.68rem;
  font-weight: 700;
  padding: 1px 7px;
  border-radius: 8px;
  background: rgba(160, 125, 224, 0.18);
  color: #a07de0;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.auto-badge {
  background: rgba(100, 200, 120, 0.15);
  color: #5dba6e;
}

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

.composer-fade-enter-active,
.composer-fade-leave-active { transition: opacity 0.15s; }
.composer-fade-enter-from,
.composer-fade-leave-to { opacity: 0; }
</style>
