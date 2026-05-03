<script setup lang="ts">
const props = defineProps<{ content: string }>()

function toLines(): string {
  try {
    const jsonPart = props.content.includes('\n') ? props.content.slice(props.content.indexOf('\n') + 1) : props.content
    const obj = JSON.parse(jsonPart)
    if (typeof obj !== 'object' || obj === null) return props.content
    return Object.entries(obj)
      .map(([k, v]) => {
        const val = typeof v === 'string' ? v : JSON.stringify(v, null, 2)
        return `${k}:\n${val}`
      })
      .join('\n\n')
  } catch {
    return props.content
  }
}
</script>

<template>
  <pre class="tool-body">{{ toLines() }}</pre>
</template>

<style scoped>
.tool-body {
  padding: 8px 12px;
  font-family: inherit;
  font-size: 0.76rem;
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--text-secondary);
  border-top: 1px solid var(--border-color);
  max-height: 320px;
  overflow-y: auto;
  margin: 0;
}
</style>
