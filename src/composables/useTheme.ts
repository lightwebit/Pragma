import { ref } from 'vue'
import { LS_THEME } from '../PRAGMA_CONSTANTS'

export function useTheme() {
  const isDark = ref(localStorage.getItem(LS_THEME) !== 'light')

  function applyTheme(dark: boolean) {
    document.documentElement.setAttribute('data-theme', dark ? 'dark' : 'light')
    localStorage.setItem(LS_THEME, dark ? 'dark' : 'light')
  }

  function toggleTheme() {
    isDark.value = !isDark.value
    applyTheme(isDark.value)
  }

  return { isDark, applyTheme, toggleTheme }
}
