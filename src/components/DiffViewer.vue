<script setup lang="ts">
interface Hunk {
  oldStart: number
  oldLines: number
  newStart: number
  newLines: number
  lines: string[]
}

const props = defineProps<{ content: string }>()

function normalizeHunk(raw: any): Hunk | null {
  const oldStart = raw.oldStart ?? raw.old_start ?? 0
  const oldLines = raw.oldLines ?? raw.old_lines ?? 0
  const newStart = raw.newStart ?? raw.new_start ?? 0
  const newLines = raw.newLines ?? raw.new_lines ?? 0
  const lines: string[] = Array.isArray(raw.lines) ? raw.lines : []
  if (!lines.length && !oldLines && !newLines) return null
  return { oldStart, oldLines, newStart, newLines, lines }
}

function parseHunks(): Hunk[] {
  try {
    let parsed = JSON.parse(props.content)
    // unwrap { hunks: [...] } or { patches: [...] } if needed
    if (parsed && !Array.isArray(parsed)) {
      parsed = parsed.hunks ?? parsed.patches ?? parsed.diffs ?? null
    }
    if (!Array.isArray(parsed)) return []
    return parsed.map(normalizeHunk).filter((h): h is Hunk => h !== null)
  } catch {
    return []
  }
}

function hunkHeader(h: Hunk): string {
  return `@@ -${h.oldStart},${h.oldLines} +${h.newStart},${h.newLines} @@`
}

function lineClass(line: string): string {
  if (line.startsWith('+')) return 'diff-added'
  if (line.startsWith('-')) return 'diff-removed'
  return 'diff-context'
}

// Fallback: content is plain text diff (lines starting with +/-/ )
function legacyLines(): string[] {
  if (parseHunks().length > 0) return []
  return props.content.split('\n').filter(l => /^[+\- ]/.test(l))
}
</script>

<template>
  <div class="diff-viewer">
    <template v-if="parseHunks().length > 0">
      <template v-for="(hunk, hi) in parseHunks()" :key="hi">
        <div class="diff-hunk-header">{{ hunkHeader(hunk) }}</div>
        <div
          v-for="(line, li) in hunk.lines"
          :key="li"
          :class="['diff-line', lineClass(line)]"
        >{{ line }}</div>
      </template>
    </template>

    <template v-else-if="legacyLines().length > 0">
      <div
        v-for="(line, i) in legacyLines()"
        :key="i"
        :class="['diff-line', lineClass(line)]"
      >{{ line }}</div>
    </template>

    <div v-else class="diff-empty">no changes</div>
  </div>
</template>

<style scoped>
.diff-viewer {
  font-family: monospace;
  font-size: 0.78rem;
  overflow-x: auto;
  max-height: 480px;
  overflow-y: auto;
  border-top: 1px solid var(--border-color);
}

.diff-line {
  padding: 1px 10px;
  white-space: pre;
  line-height: 1.5;
}

.diff-added {
  background: rgba(76, 175, 77, 0.14);
  color: #7ec87e;
}

.diff-removed {
  background: rgba(232, 90, 74, 0.14);
  color: #e08080;
}

.diff-context {
  color: var(--text-secondary);
}

.diff-hunk-header {
  padding: 2px 10px;
  background: rgba(156, 111, 214, 0.12);
  color: var(--color-atom-style);
  font-family: monospace;
  font-size: 0.72rem;
  white-space: pre;
  line-height: 1.5;
  border-top: 1px solid rgba(156, 111, 214, 0.15);
}

.diff-empty {
  padding: 6px 10px;
  font-size: 0.72rem;
  color: var(--text-secondary);
  opacity: 0.5;
  font-style: italic;
}
</style>
