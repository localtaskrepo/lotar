import { ref } from 'vue'

export interface SavedView {
  id: string
  name: string
  filter: Record<string, string>
  columns?: string[]
  sort?: { key: string | null; dir: 'asc' | 'desc' }
  created: string
  updated: string
}

// NOTE: For now, we persist per-browser only. We can later namespace by repo
// (e.g., lotar.ui.<repo_id>.savedViews) once a stable repo identifier is available.
const STORAGE_KEY = 'lotar.savedViews'

function load(): SavedView[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return []
    const arr = JSON.parse(raw)
    if (Array.isArray(arr)) return arr
  } catch {}
  return []
}

function persist(list: SavedView[]) {
  try { localStorage.setItem(STORAGE_KEY, JSON.stringify(list)) } catch {}
}

function genId(name: string) {
  const slug = name.trim().toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/(^-|-$)/g, '').slice(0, 40)
  const rand = Math.random().toString(36).slice(2, 6)
  return `${slug || 'view'}-${rand}`
}

export function useSavedViews() {
  const views = ref<SavedView[]>(load())

  function refresh() {
    views.value = load()
  }

  function saveNew(name: string, filter: Record<string, string>, extras?: { columns?: string[]; sort?: { key: string | null; dir: 'asc' | 'desc' } }) {
    const now = new Date().toISOString()
    const v: SavedView = { id: genId(name), name: name.trim() || 'View', filter: { ...filter }, columns: extras?.columns ? [...extras.columns] : undefined, sort: extras?.sort ? { ...extras.sort } : undefined, created: now, updated: now }
    const next = [v, ...views.value]
    views.value = next
    persist(next)
    return v
  }

  function updateExisting(id: string, patch: Partial<Pick<SavedView, 'name' | 'filter' | 'columns' | 'sort'>>) {
    const now = new Date().toISOString()
    const next = views.value.map(v => v.id === id ? {
      ...v,
      ...patch,
      filter: patch.filter ? { ...patch.filter } : v.filter,
      columns: patch.columns ? [...patch.columns] : v.columns,
      sort: patch.sort ? { ...patch.sort } : v.sort,
      updated: now,
    } : v)
    views.value = next
    persist(next)
    return views.value.find(v => v.id === id) || null
  }

  function remove(id: string) {
    const next = views.value.filter(v => v.id !== id)
    views.value = next
    persist(next)
  }

  function getById(id: string) {
    return views.value.find(v => v.id === id) || null
  }

  return { views, refresh, saveNew, updateExisting, remove, getById }
}
