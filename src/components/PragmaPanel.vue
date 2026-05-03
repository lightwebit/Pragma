<script setup lang="ts">
import { ref, computed, reactive, watch } from 'vue'
import { useSessionStore } from '../stores/session'
import AnalysisCard from './AnalysisCard.vue'
import QuestionsCard from './QuestionsCard.vue'
import PlanCard from './PlanCard.vue'
import ReportCard from './ReportCard.vue'
import TestsCard from './TestsCard.vue'
import ClosingCard from './ClosingCard.vue'
import RawNoteCard from './RawNoteCard.vue'

interface Profile {
  label: string
  binary: string
  configDir?: string | null
}

const props = defineProps<{ profile: Profile | null; workingDir?: string; width?: number }>()
const emit = defineEmits<{ continueSession: []; newSession: [] }>()
const store = useSessionStore()
const collapsed = ref(false)

// Per-card collapsed state — managed here for auto-collapse
const cc = reactive({
  analysis: false,
  questions: false,
  plan: false,
  report: false,
  tests: false,
  closing: false,
  rawNote: false,
})

// When a new card appears, collapse all previous ones
watch(() => store.pragmaQuestions, (val) => {
  if (val) cc.analysis = true
})
watch(() => store.pragmaPlan, (val) => {
  if (val) { cc.analysis = true; cc.questions = true }
})
watch(() => store.pragmaReport, (val) => {
  if (val) { cc.analysis = true; cc.questions = true; cc.plan = true }
})
watch(() => store.pragmaTests, (val) => {
  if (val) { cc.analysis = true; cc.questions = true; cc.plan = true; cc.report = true }
})
watch(() => store.pragmaClosing, (val) => {
  if (val) { cc.analysis = true; cc.questions = true; cc.plan = true; cc.report = true; cc.tests = true }
})

// Reset collapsed state when a new session starts
watch(() => store.sessionId, () => {
  cc.analysis = false
  cc.questions = false
  cc.plan = false
  cc.report = false
  cc.tests = false
  cc.closing = false
  cc.rawNote = false
})

const hasContent = computed(() =>
  store.pragmaAnalysis !== null
  || store.pragmaRawNote !== null
  || store.pragmaQuestions !== null
  || store.pragmaPlan !== null
  || store.pragmaReport !== null
  || store.pragmaTests !== null
  || store.pragmaClosing !== null
  || store.completedSteps.length > 0
)

const phaseLabel = computed(() => {
  switch (store.pragmaPhase) {
    case 'awaiting_answers':      return 'answers'
    case 'awaiting_approval':     return 'approval'
    case 'awaiting_confirmation': return 'confirm'
    case 'awaiting_close':        return 'done'
    default: return null
  }
})

function sendControl(msg: 'CONTINUE' | 'ABORT') {
  if (!props.profile) return
  store.sendControl(msg, props.profile.binary, props.profile.configDir ?? undefined, props.workingDir)
}
</script>

<template>
  <div v-if="store.sessionId || hasContent || store.pragmaPhase" class="pragma-panel" :class="{ 'is-collapsed': collapsed }" :style="!collapsed && props.width ? { width: props.width + 'px' } : undefined">
    <div class="panel-header">
      <button
        class="collapse-btn"
        :title="collapsed ? 'Expand' : 'Collapse'"
        @click="collapsed = !collapsed"
      >{{ collapsed ? '‹' : '›' }}</button>
      <template v-if="!collapsed">
        <span class="panel-title">pragma</span>
        <span v-if="phaseLabel" class="phase-pill">{{ phaseLabel }}</span>
      </template>
    </div>

    <template v-if="!collapsed">
      <div v-if="!hasContent && !store.pragmaPhase" class="panel-empty">
        <span v-if="store.running">waiting for plan…</span>
        <span v-else>no structured data</span>
      </div>

      <div class="panel-scroll">
        <RawNoteCard
          v-if="store.pragmaRawNote"
          :data="store.pragmaRawNote"
          :collapsed="cc.rawNote"
          @toggle="cc.rawNote = !cc.rawNote"
        />
        <AnalysisCard
          v-if="store.pragmaAnalysis"
          :data="store.pragmaAnalysis"
          :collapsed="cc.analysis"
          @toggle="cc.analysis = !cc.analysis"
        />
        <QuestionsCard
          v-if="store.pragmaQuestions"
          :data="store.pragmaQuestions"
          :phase="store.pragmaPhase"
          :profile="profile"
          :collapsed="cc.questions"
          @toggle="cc.questions = !cc.questions"
        />
        <PlanCard
          v-if="store.pragmaPlan"
          :data="store.pragmaPlan"
          :completed-steps="store.completedSteps"
          :phase="store.pragmaPhase"
          :profile="profile"
          :working-dir="workingDir"
          :collapsed="cc.plan"
          @toggle="cc.plan = !cc.plan"
        />
        <ReportCard
          v-if="store.pragmaReport"
          :data="store.pragmaReport"
          :collapsed="cc.report"
          @toggle="cc.report = !cc.report"
        />
        <TestsCard
          v-if="store.pragmaTests"
          :data="store.pragmaTests"
          :collapsed="cc.tests"
          @toggle="cc.tests = !cc.tests"
        />
        <ClosingCard
          v-if="store.pragmaClosing"
          :data="store.pragmaClosing"
          :collapsed="cc.closing"
          @toggle="cc.closing = !cc.closing"
          @continue="emit('continueSession')"
          @new-session="emit('newSession')"
        />
      </div>

      <!-- Confirmation phase controls -->
      <div v-if="store.pragmaPhase === 'awaiting_confirmation'" class="panel-footer">
        <button class="ctrl-btn ctrl-ok" :disabled="store.running" @click="sendControl('CONTINUE')">Continue</button>
        <button class="ctrl-btn ctrl-abort" :disabled="store.running" @click="sendControl('ABORT')">Cancel</button>
      </div>
    </template>
  </div>
</template>

<style scoped>
.pragma-panel {
  width: 300px;
  flex-shrink: 0;
  border-left: 1px solid var(--border-color);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  background: var(--bg-secondary);
  transition: none;
}

.pragma-panel.is-collapsed {
  width: 28px;
}

.panel-header {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 8px;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
  min-height: 37px;
}

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

.panel-title {
  font-size: 0.7rem;
  font-weight: 700;
  letter-spacing: 0.12em;
  color: var(--text-secondary);
  text-transform: uppercase;
}

.phase-pill {
  font-size: 0.65rem;
  font-weight: 600;
  padding: 1px 7px;
  border-radius: 10px;
  background: rgba(100, 180, 255, 0.15);
  color: var(--color-atom-code);
  letter-spacing: 0.04em;
}

.panel-scroll {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.panel-scroll > * {
  flex-shrink: 0;
}

.panel-empty {
  padding: 16px 10px;
  font-size: 0.72rem;
  color: var(--text-secondary);
  font-style: italic;
  opacity: 0.45;
  text-align: center;
}

.panel-footer {
  flex-shrink: 0;
  display: flex;
  gap: 6px;
  padding: 8px 10px;
  border-top: 1px solid var(--border-color);
}

.ctrl-btn {
  padding: 5px 14px;
  border-radius: 5px;
  border: none;
  font-family: inherit;
  font-size: 0.78rem;
  font-weight: 600;
  cursor: pointer;
  transition: opacity 0.12s;
}
.ctrl-ok    { background: #4caf50; color: #fff; }
.ctrl-ok:hover { opacity: 0.85; }
.ctrl-abort { background: rgba(255, 59, 48, 0.15); color: var(--color-error, #ff3b30); border: 1px solid rgba(255,59,48,0.3); }
.ctrl-abort:hover { background: rgba(255, 59, 48, 0.25); }
</style>
