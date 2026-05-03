import { defineStore } from 'pinia'
import { ref } from 'vue'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'

export interface Atom {
  id: string
  atomType: 'FileTouch' | 'Diff' | 'ToolUse' | 'Error' | 'AgentNote' | 'UserReply' | 'PragmaEvent'
  filePath: string | null
  fileType: 'Code' | 'Config' | 'Markup' | 'Style' | 'Build' | 'Data' | 'Other' | null
  action: 'Create' | 'Modify' | 'Delete' | null
  content: string
  collapsed: boolean
  source: 'Stdout' | 'Stderr'
  receivedAt: string
  sessionId: string
}

export interface SessionInfo {
  id: string
  startedAt: string
  command: string | null
  title: string | null
  workingDir: string | null
  atomCount: number
  inputTokens: number
  outputTokens: number
  cacheReadTokens: number
  cacheWriteTokens: number
  totalCostUsd: number | null
  attachments: string[]
}

// ---------------------------------------------------------------------------
// Pragma structured events
// TODO: revisit the JSON format of event payloads in a future step.
// ---------------------------------------------------------------------------
export interface PragmaAnalysis    { text: string }
export interface PragmaQuestions   { items: string[] }
export interface PragmaPlan        { steps: string[] }
export interface PragmaStepComplete { step: number; result: string }
export interface PragmaReport      { text: string }
export interface PragmaTests       { items: string[] }
export interface PragmaClosing     { text: string }
export interface PragmaRawNote     { text: string }
export type PragmaPhase = 'awaiting_answers' | 'awaiting_approval' | 'awaiting_confirmation' | 'awaiting_close'

export interface SessionUsage {
  inputTokens: number
  outputTokens: number
  cacheReadTokens: number
  cacheWriteTokens: number
  model: string
  totalCostUsd: number | null
}

export const useSessionStore = defineStore('session', () => {
  const atoms = ref<Atom[]>([])
  const sessionId = ref<string | null>(null)
  const running = ref(false)
  const newCount = ref(0)
  const scrollLocked = ref(true)
  const lastError = ref<string | null>(null)
  const stoppedByUser = ref(false)

  // Review mode: true when viewing a saved session (not live)
  const reviewMode = ref(false)

  // Saved sessions list
  const savedSessions = ref<SessionInfo[]>([])

  // Path dell'ultimo export scritto su disco
  const exportedPath = ref<string | null>(null)

  const sessionTitle = ref('')
  const sessionWorkingDir = ref('')
  const sessionAttachments = ref<string[]>([])

  // Pragma structured events — updated during the live session
  const pragmaAnalysis   = ref<PragmaAnalysis | null>(null)
  const pragmaQuestions  = ref<PragmaQuestions | null>(null)
  const pragmaPlan       = ref<PragmaPlan | null>(null)
  const pragmaPhase      = ref<PragmaPhase | null>(null)
  const pragmaReport     = ref<PragmaReport | null>(null)
  const pragmaTests      = ref<PragmaTests | null>(null)
  const pragmaClosing    = ref<PragmaClosing | null>(null)
  const pragmaRawNote    = ref<PragmaRawNote | null>(null)
  const completedSteps   = ref<PragmaStepComplete[]>([])
  const sessionUsage     = ref<SessionUsage | null>(null)
  const sessionModel     = ref<'sonnet' | 'opus'>('sonnet')
  const stepTokenStart   = ref<SessionUsage | null>(null)

  let unlistenBatch: (() => void) | null = null
  let unlistenComplete: (() => void) | null = null
  let unlistenError: (() => void) | null = null
  let unlistenDebug: (() => void) | null = null
  let unlistenAnalysis: (() => void) | null = null
  let unlistenQuestions: (() => void) | null = null
  let unlistenPlan: (() => void) | null = null
  let unlistenPhase: (() => void) | null = null
  let unlistenReport: (() => void) | null = null
  let unlistenTests: (() => void) | null = null
  let unlistenStepComplete: (() => void) | null = null
  let unlistenUsage: (() => void) | null = null
  let unlistenClosing: (() => void) | null = null
  let unlistenRawNote: (() => void) | null = null

  async function startSession(
    prompt: string,
    binary: string,
    configDir?: string,
    workingDir?: string,
    profileLabel?: string,
    title?: string,
    model?: 'sonnet' | 'opus',
    resumeId?: string,
    attachments?: string[],
    stepByStep?: boolean,
  ) {
    unlistenBatch?.()
    unlistenComplete?.()
    unlistenError?.()
    unlistenDebug?.()
    unlistenAnalysis?.()
    unlistenQuestions?.()
    unlistenPlan?.()
    unlistenPhase?.()
    unlistenReport?.()
    unlistenTests?.()
    unlistenStepComplete?.()
    unlistenUsage?.()
    unlistenClosing?.()
    unlistenRawNote?.()

    if (atoms.value.length > 0) {
      atoms.value.push({
        id: crypto.randomUUID(),
        atomType: 'UserReply',
        filePath: null,
        fileType: null,
        action: null,
        content: `\x00separator`,
        collapsed: false,
        source: 'Stdout',
        receivedAt: new Date().toISOString(),
        sessionId: sessionId.value ?? '',
      })
    }
    newCount.value = 0
    scrollLocked.value = true
    running.value = true
    lastError.value = null
    reviewMode.value = false
    if (!resumeId) {
      sessionTitle.value = title || ''
      sessionModel.value = model ?? 'sonnet'
    } else if (model) {
      sessionModel.value = model
    }
    pragmaAnalysis.value = null
    pragmaQuestions.value = null
    pragmaPlan.value = null
    pragmaPhase.value = null
    pragmaReport.value = null
    pragmaTests.value = null
    pragmaClosing.value = null
    pragmaRawNote.value = null
    completedSteps.value = []
    sessionUsage.value = null
    stepTokenStart.value = null

    unlistenBatch = await listen<Atom[]>('atoms:batch', (event) => {
      const incoming = event.payload
      atoms.value.push(...incoming)
      if (!scrollLocked.value) {
        newCount.value += incoming.length
      }
    })

    unlistenComplete = await listen<string>('session:complete', () => {
      running.value = false
      fetchSavedSessions()
    })

    unlistenError = await listen<{ code: string; message: string }>('session:error', (event) => {
      if (stoppedByUser.value) {
        stoppedByUser.value = false
      } else {
        lastError.value = event.payload.message
      }
      running.value = false
      fetchSavedSessions()
    })

    unlistenDebug = await listen<string>('session:debug', (event) => {
      console.log('[pragma debug]', event.payload)
    })

    unlistenAnalysis = await listen<PragmaAnalysis>('pragma:analysis', (e) => {
      pragmaAnalysis.value = e.payload
      pushLocalAtom(`ANALYSIS\n${e.payload.text}`, 'PragmaEvent')
    })
    unlistenQuestions = await listen<PragmaQuestions>('pragma:questions', (e) => {
      pragmaQuestions.value = e.payload
      pushLocalAtom(`QUESTIONS\n${e.payload.items.map((q, i) => `${i + 1}. ${q}`).join('\n')}`, 'PragmaEvent')
    })
    unlistenPlan = await listen<PragmaPlan>('pragma:plan', (e) => {
      pragmaPlan.value = e.payload
      pushLocalAtom(`PLAN\n${e.payload.steps.join('\n')}`, 'PragmaEvent')
    })
    unlistenPhase = await listen<PragmaPhase>('pragma:phase', (e) => {
      pragmaPhase.value = e.payload
    })
    unlistenReport = await listen<PragmaReport>('pragma:report', (e) => {
      pragmaReport.value = e.payload
      pushLocalAtom(`REPORT\n${e.payload.text}`, 'PragmaEvent')
    })
    unlistenTests = await listen<PragmaTests>('pragma:tests', (e) => {
      pragmaTests.value = e.payload
      pushLocalAtom(`TESTS\n${e.payload.items.join('\n')}`, 'PragmaEvent')
    })
    unlistenStepComplete = await listen<PragmaStepComplete>('pragma:step_complete', (e) => {
      completedSteps.value.push(e.payload)
      pushLocalAtom(`STEP ${e.payload.step}\n${e.payload.result}`, 'PragmaEvent')
      stepTokenStart.value = sessionUsage.value ? { ...sessionUsage.value } : null
    })
    unlistenUsage = await listen<SessionUsage>('session:usage', (e) => {
      sessionUsage.value = e.payload
    })
    unlistenClosing = await listen<PragmaClosing>('pragma:closing', (e) => {
      pragmaClosing.value = e.payload
      pushLocalAtom(`CLOSING\n${e.payload.text}`, 'PragmaEvent')
    })
    unlistenRawNote = await listen<PragmaRawNote>('pragma:raw_note', (e) => {
      pragmaRawNote.value = e.payload
      pushLocalAtom(`RAW_NOTE\n${e.payload.text}`, 'PragmaEvent')
    })

    // Push prompt as first visible atom before invoke
    const promptAtom: Atom = {
      id: crypto.randomUUID(),
      atomType: 'UserReply',
      filePath: null,
      fileType: null,
      action: null,
      content: prompt,
      collapsed: false,
      source: 'Stdout',
      receivedAt: new Date().toISOString(),
      sessionId: '',
    }
    atoms.value.push(promptAtom)

    try {
      sessionId.value = await invoke<string>('run_pragma', {
        prompt,
        binary,
        configDir: configDir ?? null,
        workingDir: workingDir ?? null,
        profileLabel: profileLabel ?? null,
        title: title ?? null,
        model: model ?? null,
        attachments: attachments ?? [],
        existingSessionId: resumeId ?? null,
        stepByStep: stepByStep ?? false,
      })
      promptAtom.sessionId = sessionId.value
      invoke('save_local_atom', { atom: promptAtom }).catch(() => {})
      if (!resumeId) {
        // New session: inject at the top of the list immediately
        savedSessions.value = [
          {
            id: sessionId.value,
            title: null,
            startedAt: new Date().toISOString(),
            command: prompt,
            workingDir: workingDir ?? null,
            atomCount: 0,
            inputTokens: 0, outputTokens: 0, cacheReadTokens: 0, cacheWriteTokens: 0,
            totalCostUsd: null,
            attachments: attachments ?? [],
          },
          ...savedSessions.value.filter(s => s.id !== sessionId.value),
        ]
      }
    } catch (e) {
      lastError.value = String(e)
      running.value = false
    }
  }

  /** Restores structured pragma fields from PragmaEvent atoms of a loaded session. */
  function restorePragmaFromAtoms(atomList: Atom[]) {
    pragmaAnalysis.value = null
    pragmaQuestions.value = null
    pragmaPlan.value = null
    pragmaPhase.value = null
    pragmaReport.value = null
    pragmaTests.value = null
    pragmaClosing.value = null
    pragmaRawNote.value = null
    completedSteps.value = []

    for (const atom of atomList) {
      if (atom.atomType !== 'PragmaEvent') continue
      const body = atom.content
      if (body.startsWith('ANALYSIS\n')) {
        pragmaAnalysis.value = { text: body.slice('ANALYSIS\n'.length).trim() }
      } else if (body.startsWith('QUESTIONS\n')) {
        const items = body.slice('QUESTIONS\n'.length).split('\n')
          .map(l => l.replace(/^\d+\.\s*/, '').trim()).filter(Boolean)
        if (items.length) pragmaQuestions.value = { items }
      } else if (body.startsWith('PLAN\n')) {
        const steps = body.slice('PLAN\n'.length).split('\n')
          .map(l => l.replace(/^[-*]\s+/, '').trim())
          .filter(l => l && !/^-+$/.test(l))
        if (steps.length) pragmaPlan.value = { steps }
      } else if (/^STEP \d+\n/.test(body)) {
        const m = body.match(/^STEP (\d+)\n(.*)$/s)
        if (m) completedSteps.value.push({ step: parseInt(m[1]), result: m[2].trim() })
      } else if (body.startsWith('REPORT\n')) {
        pragmaReport.value = { text: body.slice('REPORT\n'.length).trim() }
      } else if (body.startsWith('TESTS\n')) {
        const items = body.slice('TESTS\n'.length).split('\n')
          .map(l => l.replace(/^-\s*/, '').trim()).filter(l => l && !/^-+$/.test(l))
        if (items.length) pragmaTests.value = { items }
      } else if (body.startsWith('CLOSING\n')) {
        pragmaClosing.value = { text: body.slice('CLOSING\n'.length).trim() }
      } else if (body.startsWith('RAW_NOTE\n')) {
        pragmaRawNote.value = { text: body.slice('RAW_NOTE\n'.length).trim() }
      }
    }
  }

  /** Loads a saved session in review mode (no live stream). */
  async function loadSession(id: string) {
    if (running.value && sessionId.value) {
      await invoke('kill_session', { sessionId: sessionId.value }).catch(() => {})
      running.value = false
    }
    atoms.value = []
    newCount.value = 0
    scrollLocked.value = true
    lastError.value = null
    reviewMode.value = false
    sessionId.value = id
    const si = savedSessions.value.find(s => s.id === id)
    sessionTitle.value = si?.title || ''
    sessionWorkingDir.value = si?.workingDir || ''
    sessionAttachments.value = si?.attachments ?? []
    sessionUsage.value = si
      ? { inputTokens: si.inputTokens, outputTokens: si.outputTokens,
          cacheReadTokens: si.cacheReadTokens, cacheWriteTokens: si.cacheWriteTokens,
          model: '', totalCostUsd: si.totalCostUsd }
      : null

    try {
      const loaded = await invoke<Atom[]>('load_session', { sessionId: id })
      atoms.value = loaded
      restorePragmaFromAtoms(loaded)
    } catch (e) {
      lastError.value = String(e)
    }
  }

  /** Loads the list of saved sessions from the DB. */
  async function fetchSavedSessions() {
    try {
      const fromDb = await invoke<SessionInfo[]>('list_sessions')
      // Keep running session visible even if not yet saved to DB
      if (running.value && sessionId.value && !fromDb.find(s => s.id === sessionId.value)) {
        const optimistic = savedSessions.value.find(s => s.id === sessionId.value)
        savedSessions.value = optimistic ? [optimistic, ...fromDb] : fromDb
      } else {
        savedSessions.value = fromDb
      }
    } catch (e) {
      console.error('list_sessions:', e)
    }
  }

  /** Full-text search over saved sessions. */
  async function searchSessions(query: string): Promise<SessionInfo[]> {
    try {
      return await invoke<SessionInfo[]>('search_sessions', { query })
    } catch (e) {
      console.error('search_sessions:', e)
      return []
    }
  }

  /** Saves the current session and reloads the list. */
  async function saveCurrentSession() {
    if (!sessionId.value) return
    await invoke('save_session', {
      sessionId: sessionId.value,
      title: sessionTitle.value || null,
    })
    await fetchSavedSessions()
  }

  /** Duplicates the current session in the DB and opens it in review mode. */
  async function duplicateCurrentSession() {
    if (!sessionId.value) return
    try {
      const newId = await invoke<string>('duplicate_session', { sessionId: sessionId.value })
      await fetchSavedSessions()
      await loadSession(newId)
    } catch (e) {
      lastError.value = String(e)
    }
  }

  /** Writes the session JSON to ~/.pragma/exports/ and returns the path. */
  async function exportCurrentSession(): Promise<string | null> {
    if (!sessionId.value) return null
    try {
      const path = await invoke<string>('export_session_to_file', { sessionId: sessionId.value })
      exportedPath.value = path
      return path
    } catch (e) {
      lastError.value = String(e)
      return null
    }
  }

  /** Generates the session Markdown and writes it to ~/.pragma/exports/. */
  async function exportCurrentSessionMarkdown(workingDir?: string): Promise<string | null> {
    if (!sessionId.value) return null
    try {
      const dbAtoms = await invoke<Atom[]>('load_session', { sessionId: sessionId.value })
      const md = buildMarkdown(dbAtoms, sessionId.value, sessionTitle.value, sessionAttachments.value, sessionModel.value, workingDir)
      const path = await invoke<string>('export_session_to_markdown', {
        sessionId: sessionId.value,
        content: md,
      })
      exportedPath.value = path
      return path
    } catch (e) {
      lastError.value = String(e)
      return null
    }
  }

  function buildMarkdown(
    atomList: Atom[],
    sid: string,
    title: string,
    attachments?: string[],
    model?: string,
    workingDir?: string,
  ): string {
    const lines: string[] = []
    const heading = title || 'Pragma Session'
    lines.push(`# ${heading}`)
    lines.push(`**Session:** \`${sid}\`  `)
    lines.push(`**Date:** ${new Date().toISOString().slice(0, 10)}`)
    if (model) lines.push(`**Model:** ${model}`)
    if (workingDir) lines.push(`**Working dir:** \`${workingDir}\``)
    if (attachments && attachments.length > 0)
      lines.push(`**Attachments:** ${attachments.map(a => `\`${a}\``).join(', ')}`)
    lines.push('')

    const pragma = atomList.filter(a => a.atomType === 'PragmaEvent')
    const userReplies = atomList.filter(a => a.atomType === 'UserReply')
    const agentNotes = atomList.filter(a => a.atomType === 'AgentNote')
    const errors = atomList.filter(a => a.atomType === 'Error')
    const toolCount = atomList.filter(a => ['FileTouch', 'Diff', 'ToolUse'].includes(a.atomType)).length

    if (userReplies.length > 0) {
      lines.push('---', '', '## Prompt', '')
      lines.push(userReplies[0].content.trim())
      for (const r of userReplies.slice(1)) {
        lines.push('', `**User reply:** ${r.content.trim()}`)
      }
      lines.push('')
    }

    for (const atom of pragma) {
      const body = atom.content
      if (body.startsWith('ANALYSIS\n')) {
        lines.push('---', '', '## Analysis', '', body.slice('ANALYSIS\n'.length).trim(), '')
      } else if (body.startsWith('QUESTIONS\n')) {
        lines.push('---', '', '## Questions', '', body.slice('QUESTIONS\n'.length).trim(), '')
      } else if (body.startsWith('PLAN\n')) {
        lines.push('---', '', '## Plan', '', body.slice('PLAN\n'.length).trim(), '')
      } else if (body.startsWith('REPORT\n')) {
        lines.push('---', '', '## Report', '', body.slice('REPORT\n'.length).trim(), '')
      } else if (/^STEP \d+\n/.test(body)) {
        const match = body.match(/^(STEP \d+)\n(.*)$/s)
        if (match) lines.push('', `### ${match[1]}`, '', match[2].trim(), '')
      } else if (body.startsWith('RAW_NOTE\n')) {
        lines.push('---', '', '## Response', '', body.slice('RAW_NOTE\n'.length).trim(), '')
      }
    }

    // If Claude responded without pragma markers and no raw_note, include the raw agent notes
    if (pragma.length === 0 && agentNotes.length > 0) {
      lines.push('---', '', '## Response', '')
      for (const note of agentNotes) {
        const text = note.content.trim()
        if (text) lines.push(text, '')
      }
    }

    if (errors.length > 0) {
      lines.push('---', '', '## Errors', '')
      for (const e of errors) lines.push(`> ⚠ ${e.content.trim()}`, '')
    }

    if (toolCount > 0) {
      lines.push('---', '', `*File operations: ${toolCount}*`, '')
    }

    return lines.join('\n')
  }

  /** Sends a control message to a waiting session (--resume). */
  async function sendControl(
    message: 'APPROVED' | 'CONTINUE' | 'FINALIZE' | 'ABORT' | 'SKIP_QUESTIONS',
    binary: string,
    configDir?: string,
    workingDir?: string,
  ) {
    if (!sessionId.value) { console.warn('[sendControl] sessionId is null — ignoring', message); return }
    const labelMap: Record<string, string> = {
      APPROVED: '✓ Plan approved',
      CONTINUE: '→ Continue',
      FINALIZE: '→ Finalize',
      ABORT: '✗ Abort',
      SKIP_QUESTIONS: '→ Skip questions',
    }
    pushLocalAtom(labelMap[message] ?? `→ ${message}`, 'UserReply')
    pragmaPhase.value = null
    running.value = true
    try {
      await invoke('send_control', {
        sessionId: sessionId.value,
        message,
        binary,
        configDir: configDir ?? null,
        workingDir: workingDir ?? null,
        model: sessionModel.value,
      })
    } catch (e) {
      lastError.value = String(e)
      running.value = false
    }
  }

  /** Sends answers to blocking questions. */
  async function sendAnswers(
    answers: Record<string, string>,
    binary: string,
    configDir?: string,
    workingDir?: string,
  ) {
    if (!sessionId.value) return
    const msg = 'ANSWERS: ' + Object.entries(answers).map(([k, v]) => `${k}=${v}`).join(' ')
    const answersDisplay = Object.entries(answers).map(([k, v]) => `${k}: ${v}`).join('\n')
    pushLocalAtom(answersDisplay, 'UserReply')
    pragmaPhase.value = null
    running.value = true
    try {
      await invoke('send_control', {
        sessionId: sessionId.value,
        message: msg,
        binary,
        configDir: configDir ?? null,
        workingDir: workingDir ?? null,
      })
    } catch (e) {
      lastError.value = String(e)
      running.value = false
    }
  }

  /** Sends plan modification instructions. */
  async function sendModifyPlan(
    instructions: string,
    binary: string,
    configDir?: string,
    workingDir?: string,
  ) {
    if (!sessionId.value) { console.warn('[sendModifyPlan] sessionId is null — ignoring'); return }
    running.value = true
    try {
      await invoke('send_control', {
        sessionId: sessionId.value,
        message: `MODIFY_PLAN: ${instructions}`,
        binary,
        configDir: configDir ?? null,
        workingDir: workingDir ?? null,
      })
    } catch (e) {
      lastError.value = String(e)
      running.value = false
    }
  }

  function pushLocalAtom(content: string, atomType: Atom['atomType'] = 'UserReply') {
    const atom: Atom = {
      id: crypto.randomUUID(),
      atomType,
      filePath: null,
      fileType: null,
      action: null,
      content,
      collapsed: false,
      source: 'Stdout',
      receivedAt: new Date().toISOString(),
      sessionId: sessionId.value ?? '',
    }
    atoms.value.push(atom)
    if (sessionId.value) {
      invoke('save_local_atom', { atom }).catch(() => {})
    }
  }

  async function deleteSession(id: string) {
    try {
      await invoke('delete_session', { sessionId: id })
      savedSessions.value = savedSessions.value.filter(s => s.id !== id)
      if (sessionId.value === id) {
        await newSession()
      }
    } catch (e) {
      lastError.value = String(e)
    }
  }

  async function stopSession() {
    if (!sessionId.value || !running.value) return
    stoppedByUser.value = true
    try {
      await invoke('kill_session', { sessionId: sessionId.value })
    } catch (e) {
      console.error('kill_session:', e)
      stoppedByUser.value = false
    }
    pragmaPhase.value = null
    running.value = false
  }

  function toggleCollapse(atomId: string) {
    const atom = atoms.value.find(a => a.id === atomId)
    if (atom) atom.collapsed = !atom.collapsed
  }

  function setScrollLocked(locked: boolean) {
    scrollLocked.value = locked
    if (locked) newCount.value = 0
  }

  async function newSession() {
    if (running.value && sessionId.value) {
      await invoke('kill_session', { sessionId: sessionId.value }).catch(() => {})
    }
    atoms.value = []
    sessionId.value = null
    sessionTitle.value = ''
    running.value = false
    newCount.value = 0
    lastError.value = null
    reviewMode.value = false
    pragmaAnalysis.value = null
    pragmaQuestions.value = null
    pragmaPlan.value = null
    pragmaPhase.value = null
    pragmaReport.value = null
    pragmaTests.value = null
    pragmaClosing.value = null
    completedSteps.value = []
    sessionUsage.value = null
    stepTokenStart.value = null
    sessionWorkingDir.value = ''
    sessionAttachments.value = []
    exportedPath.value = null
  }

  return {
    atoms,
    sessionId,
    sessionTitle,
    running,
    newCount,
    scrollLocked,
    lastError,
    reviewMode,
    savedSessions,
    exportedPath,
    pragmaAnalysis,
    pragmaQuestions,
    pragmaPlan,
    pragmaPhase,
    pragmaReport,
    pragmaTests,
    pragmaClosing,
    pragmaRawNote,
    completedSteps,
    sessionUsage,
    sessionModel,
    stepTokenStart,
    sessionWorkingDir,
    sessionAttachments,
    startSession,
    restorePragmaFromAtoms,
    deleteSession,
    stopSession,
    pushLocalAtom,
    sendControl,
    sendAnswers,
    sendModifyPlan,
    loadSession,
    fetchSavedSessions,
    searchSessions,
    saveCurrentSession,
    duplicateCurrentSession,
    exportCurrentSession,
    exportCurrentSessionMarkdown,
    toggleCollapse,
    setScrollLocked,
    newSession,
  }
})
