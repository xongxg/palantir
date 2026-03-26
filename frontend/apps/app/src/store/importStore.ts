import { create } from 'zustand'
import type { Dataset, EntityType } from '@/api/types'

export interface ColumnDef {
  name: string
  editedName: string
  inferredType: string
  samples: string[]
  ignored: boolean
  isPk: boolean
}

export interface LinkRow {
  fkCol: string
  toEntityTypeId: string
  relType: string
}

interface ImportState {
  // data
  datasets: Dataset[]
  entityTypes: EntityType[]
  // selection
  selectedId: string | null
  expandedGroups: Set<string>      // ET IDs whose group is expanded
  // current dataset state
  columns: ColumnDef[]
  savedPk: string
  syncMode: string
  links: LinkRow[]
  savedMapping: Record<string, string>
  // promote
  promoting: boolean
  promoteResult: { ok: boolean; message: string } | null

  // actions
  setDatasets: (ds: Dataset[]) => void
  setEntityTypes: (ets: EntityType[]) => void
  selectDataset: (id: string) => void
  toggleGroup: (etId: string) => void
  setColumns: (cols: ColumnDef[]) => void
  updateColumn: (name: string, patch: Partial<ColumnDef>) => void
  setSyncMode: (mode: string) => void
  setLinks: (links: LinkRow[]) => void
  addLink: (link: LinkRow) => void
  removeLink: (idx: number) => void
  updateLink: (idx: number, patch: Partial<LinkRow>) => void
  setSavedMapping: (pk: string, map: Record<string, string>) => void
  setPromoting: (v: boolean) => void
  setPromoteResult: (r: { ok: boolean; message: string } | null) => void
  markDatasetPromoted: (datasetId: string, etId: string) => void
}

export const useImportStore = create<ImportState>((set) => ({
  datasets: [],
  entityTypes: [],
  selectedId: null,
  expandedGroups: new Set(),
  columns: [],
  savedPk: '',
  syncMode: 'snapshot',
  links: [],
  savedMapping: {},
  promoting: false,
  promoteResult: null,

  setDatasets:    ds  => set({ datasets: ds }),
  setEntityTypes: ets => set({ entityTypes: ets }),

  selectDataset: id => set({
    selectedId: id, columns: [], savedPk: '', syncMode: 'snapshot',
    links: [], savedMapping: {}, promoteResult: null,
  }),

  toggleGroup: etId => set(s => {
    const next = new Set(s.expandedGroups)
    next.has(etId) ? next.delete(etId) : next.add(etId)
    return { expandedGroups: next }
  }),

  setColumns: cols => set({ columns: cols }),
  updateColumn: (name, patch) => set(s => ({
    columns: s.columns.map(c => c.name === name ? { ...c, ...patch } : c),
  })),

  setSyncMode: mode => set({ syncMode: mode }),

  setLinks:         links => set({ links }),
  addLink:          link  => set(s => ({ links: [...s.links, link] })),
  removeLink:       idx   => set(s => ({ links: s.links.filter((_, i) => i !== idx) })),
  updateLink: (idx, patch) => set(s => ({
    links: s.links.map((l, i) => i === idx ? { ...l, ...patch } : l),
  })),

  setSavedMapping: (pk, map) => set({ savedPk: pk, savedMapping: map }),

  setPromoting:     v => set({ promoting: v }),
  setPromoteResult: r => set({ promoteResult: r }),

  markDatasetPromoted: (datasetId, etId) => set(s => ({
    datasets: s.datasets.map(d => d.id === datasetId ? { ...d, entity_type_id: etId } : d),
  })),
}))
