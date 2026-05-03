import { describe, it, expect } from 'vitest'
import { isPragmaNote, filterAtoms, SIGNAL_TYPES } from './atomFilter'
import type { Atom } from './stores/session'

function makeAtom(atomType: Atom['atomType'], content: string): Atom {
  return { id: 'test-id', sessionId: 'sess', atomType, content, collapsed: false, source: 'Stdout', receivedAt: '', filePath: null, fileType: null, action: null }
}

describe('isPragmaNote', () => {
  it('returns false for non-AgentNote atoms', () => {
    expect(isPragmaNote(makeAtom('ToolUse', '## ANALYSIS: something'))).toBe(false)
    expect(isPragmaNote(makeAtom('Error', 'PLAN: foo'))).toBe(false)
  })

  it('returns false for AgentNote without pragma markers', () => {
    expect(isPragmaNote(makeAtom('AgentNote', 'Just a regular note'))).toBe(false)
    expect(isPragmaNote(makeAtom('AgentNote', ''))).toBe(false)
  })

  it('detects ANALYSIS marker', () => {
    expect(isPragmaNote(makeAtom('AgentNote', '## ANALYSIS: doing things'))).toBe(true)
  })

  it('detects QUESTIONS marker', () => {
    expect(isPragmaNote(makeAtom('AgentNote', 'QUESTIONS: what?'))).toBe(true)
  })

  it('detects PLAN marker', () => {
    expect(isPragmaNote(makeAtom('AgentNote', '# PLAN\n- step 1'))).toBe(true)
  })

  it('detects REPORT marker', () => {
    expect(isPragmaNote(makeAtom('AgentNote', '* REPORT: done'))).toBe(true)
  })

  it('detects AWAITING_ANSWERS marker', () => {
    expect(isPragmaNote(makeAtom('AgentNote', 'AWAITING_ANSWERS: waiting'))).toBe(true)
  })
})

describe('filterAtoms — raw mode (focusMode=false)', () => {
  it('hides pragma-marker AgentNotes', () => {
    const atoms = [
      makeAtom('AgentNote', '## ANALYSIS: something'),
      makeAtom('AgentNote', 'plain note'),
      makeAtom('ToolUse', 'bash command'),
    ]
    const result = filterAtoms(atoms, false)
    expect(result).toHaveLength(2)
    expect(result.map(a => a.content)).toEqual(['plain note', 'bash command'])
  })

  it('shows all non-pragma atoms regardless of type', () => {
    const atoms = SIGNAL_TYPES.map(t => makeAtom(t, 'content'))
    const nonSignal = [makeAtom('FileTouch', 'x'), makeAtom('Diff', 'y'), makeAtom('ToolUse', 'z')]
    const result = filterAtoms([...atoms, ...nonSignal], false)
    expect(result).toHaveLength(atoms.length + nonSignal.length)
  })
})

describe('filterAtoms — focus mode (focusMode=true)', () => {
  it('keeps only SIGNAL_TYPES atoms', () => {
    const atoms = [
      makeAtom('AgentNote', 'plain note'),
      makeAtom('PragmaEvent', '{}'),
      makeAtom('UserReply', 'answer'),
      makeAtom('Error', 'oops'),
      makeAtom('FileTouch', 'file.ts'),
      makeAtom('Diff', 'diff'),
      makeAtom('ToolUse', 'bash'),
    ]
    const result = filterAtoms(atoms, true)
    const types = result.map(a => a.atomType)
    expect(types).toContain('AgentNote')
    expect(types).toContain('PragmaEvent')
    expect(types).toContain('UserReply')
    expect(types).toContain('Error')
    expect(types).not.toContain('FileTouch')
    expect(types).not.toContain('Diff')
    expect(types).not.toContain('ToolUse')
  })

  it('also hides pragma-marker AgentNotes in focus mode', () => {
    const atoms = [
      makeAtom('AgentNote', '## ANALYSIS: something'), // pragma marker — hidden
      makeAtom('AgentNote', 'visible note'),            // signal + no marker — shown
    ]
    const result = filterAtoms(atoms, true)
    expect(result).toHaveLength(1)
    expect(result[0].content).toBe('visible note')
  })
})
