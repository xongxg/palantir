import { create } from 'zustand'
import type { EntityType } from '@/api/types'

interface SchemaState {
  entityTypes: EntityType[]
  selectedId: string | null
  setEntityTypes: (ets: EntityType[]) => void
  upsertEntityType: (et: EntityType) => void
  removeEntityType: (id: string) => void
  select: (id: string | null) => void
}

export const useSchemaStore = create<SchemaState>((set) => ({
  entityTypes: [],
  selectedId: null,

  setEntityTypes: ets  => set({ entityTypes: ets }),
  select:         id   => set({ selectedId: id }),

  upsertEntityType: et => set(s => ({
    entityTypes: s.entityTypes.find(e => e.id === et.id)
      ? s.entityTypes.map(e => e.id === et.id ? et : e)
      : [...s.entityTypes, et],
  })),

  removeEntityType: id => set(s => ({
    entityTypes: s.entityTypes.filter(e => e.id !== id),
    selectedId: s.selectedId === id ? null : s.selectedId,
  })),
}))
