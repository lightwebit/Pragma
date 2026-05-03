import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface Profile {
  label: string
  binary: string
  configDir: string | null
  language: string | null
}

export interface Settings {
  profiles: Profile[]
  dbPath: string | null
  notificationsEnabled: boolean
  defaultProfileIndex: number
  exportDir: string | null
  diffAlwaysOpen: boolean
  lastWorkingDir: string | null
}

export const useSettingsStore = defineStore('settings', () => {
  const settings = ref<Settings>({ profiles: [], dbPath: null, notificationsEnabled: false, defaultProfileIndex: 0, exportDir: null, diffAlwaysOpen: false, lastWorkingDir: null })
  const loaded = ref(false)
  const savedAt = ref(0)

  async function load() {
    try {
      settings.value = await invoke<Settings>('get_settings')
      loaded.value = true
    } catch (e) {
      console.error('get_settings:', e)
    }
  }

  async function save() {
    await invoke('save_settings_cmd', { settings: settings.value })
    savedAt.value++
  }

  async function detectClaude(): Promise<string | null> {
    return invoke<string | null>('detect_claude')
  }

  function addProfile(profile: Profile) {
    settings.value.profiles.push(profile)
  }

  function removeProfile(index: number) {
    settings.value.profiles.splice(index, 1)
  }

  function updateProfile(index: number, profile: Profile) {
    settings.value.profiles[index] = profile
  }

  return {
    settings,
    loaded,
    savedAt,
    load,
    save,
    detectClaude,
    addProfile,
    removeProfile,
    updateProfile,
  }
})
