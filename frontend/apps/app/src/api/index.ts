export { api } from './client'
export * from './types'

import { api } from './client'
import type { Project, Fold, Dataset, EntityType, OntologyGraph, DatasetMapping, LinkTypeMapping } from './types'

// ── Projects ──────────────────────────────────────────────────────────────────
export const projectsApi = {
  list:   ()           => api.get<{ projects: Project[] }>('/api/projects').then(r => r.projects),
  create: (name: string) => api.post<Project>('/api/projects', { name }),
  delete: (id: string)   => api.delete<void>(`/api/projects/${id}`),
  folds:  (id: string)   => api.get<{ folds: Fold[] }>(`/api/projects/${id}/folds`).then(r => r.folds),
}

// ── Folds ─────────────────────────────────────────────────────────────────────
export const foldsApi = {
  create: (projectId: string, name: string, description?: string) =>
    api.post<Fold>(`/api/projects/${projectId}/folds`, { name, description }),
  delete: (id: string) => api.delete<void>(`/api/folds/${id}`),
}

// ── Datasets ──────────────────────────────────────────────────────────────────
export const datasetsApi = {
  list:      ()           => api.get<{ datasets: Dataset[] }>('/api/datasets').then(r => r.datasets),
  getMapping: (id: string) => api.get<{ mapping: DatasetMapping | null }>(`/api/datasets/${id}/mapping`),
  saveMapping: (id: string, body: Partial<DatasetMapping> & { fold_id?: string }) =>
    api.post<void>(`/api/datasets/${id}/mapping`, body),
  getLinks:  (id: string)  => api.get<{ mappings: LinkTypeMapping[] }>(`/api/datasets/${id}/mapping`),
  records:   (id: string, limit = 3) =>
    api.get<{ records: Record<string, unknown>[] }>(`/api/datasets/${id}/records?limit=${limit}`),
  promote:   (id: string, body: { entity_type_id?: string; new_entity_type?: string; sync_mode: string; fold_id?: string }) =>
    api.post<{ promoted: number; linked: number }>(`/api/datasets/${id}/promote`, body),
}

// ── Entity Types ──────────────────────────────────────────────────────────────
export const entityTypesApi = {
  list:   ()  => api.get<{ entity_types: EntityType[] }>('/api/ontology/schema').then(r => r.entity_types),
  create: (body: { name: string; display_name: string; color?: string; icon?: string; fold_id?: string; ddd_role?: string }) =>
    api.post<EntityType>('/api/ontology/schema', body),
  delete: (id: string) => api.delete<void>(`/api/ontology/schema/${id}`),
  addField: (etId: string, body: { name: string; data_type: string; is_required?: boolean; classification?: string }) =>
    api.post<void>(`/api/ontology/schema/${etId}/fields`, body),
  deleteField: (fieldId: string) => api.delete<void>(`/api/ontology/fields/${fieldId}`),
}

// ── Graph ─────────────────────────────────────────────────────────────────────
export const graphApi = {
  get: (projectId?: string) =>
    api.get<OntologyGraph>(`/api/ontology/graph${projectId ? `?project_id=${projectId}` : ''}`),
}
