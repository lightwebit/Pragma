import { ref } from 'vue'
import {
  PANEL_LEFT_DEFAULT_PX, PANEL_RIGHT_DEFAULT_PX,
  PANEL_LEFT_MIN_PX, PANEL_LEFT_MAX_PX,
  PANEL_RIGHT_MIN_PX, PANEL_RIGHT_MAX_PX,
  LS_LEFT_WIDTH, LS_RIGHT_WIDTH,
} from '../PRAGMA_CONSTANTS'

export function usePanelResize() {
  const leftWidth = ref(parseInt(localStorage.getItem(LS_LEFT_WIDTH) || String(PANEL_LEFT_DEFAULT_PX)))
  const rightWidth = ref(parseInt(localStorage.getItem(LS_RIGHT_WIDTH) || String(PANEL_RIGHT_DEFAULT_PX)))

  let dragSide: 'left' | 'right' | null = null
  let dragStartX = 0
  let dragStartWidth = 0

  function onDrag(e: MouseEvent) {
    if (!dragSide) return
    const delta = e.clientX - dragStartX
    if (dragSide === 'left') {
      leftWidth.value = Math.max(PANEL_LEFT_MIN_PX, Math.min(PANEL_LEFT_MAX_PX, dragStartWidth + delta))
      localStorage.setItem(LS_LEFT_WIDTH, String(leftWidth.value))
    } else {
      rightWidth.value = Math.max(PANEL_RIGHT_MIN_PX, Math.min(PANEL_RIGHT_MAX_PX, dragStartWidth - delta))
      localStorage.setItem(LS_RIGHT_WIDTH, String(rightWidth.value))
    }
  }

  function stopDrag() {
    dragSide = null
    document.removeEventListener('mousemove', onDrag)
    document.removeEventListener('mouseup', stopDrag)
    document.body.style.cursor = ''
    document.body.style.userSelect = ''
  }

  function startDrag(side: 'left' | 'right', e: MouseEvent) {
    e.preventDefault()
    dragSide = side
    dragStartX = e.clientX
    dragStartWidth = side === 'left' ? leftWidth.value : rightWidth.value
    document.addEventListener('mousemove', onDrag)
    document.addEventListener('mouseup', stopDrag)
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'
  }

  return { leftWidth, rightWidth, startDrag }
}
