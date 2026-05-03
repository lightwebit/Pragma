import type { Atom } from './stores/session'

export const PRAGMA_MARKER_RE =
  /^[#*\s]*(ANALYSIS|QUESTIONS|PLAN|REPORT|STEP_COMPLETE|AWAITING_ANSWERS|AWAITING_APPROVAL|AWAITING_CONFIRMATION)[*:\s]/m

export const SIGNAL_TYPES: Atom['atomType'][] = ['PragmaEvent', 'UserReply', 'Error', 'AgentNote']

export function isPragmaNote(atom: Atom): boolean {
  if (atom.atomType !== 'AgentNote') return false
  return PRAGMA_MARKER_RE.test(atom.content)
}

export function filterAtoms(atoms: Atom[], focusMode: boolean): Atom[] {
  return atoms.filter(a => {
    if (isPragmaNote(a)) return false
    if (focusMode && !SIGNAL_TYPES.includes(a.atomType)) return false
    return true
  })
}
