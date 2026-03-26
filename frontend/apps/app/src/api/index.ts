export { api } from './client'
export * from './types'

import { api } from './client'
import type { Project, Fold, Dataset, DataSource, SyncJob, EntityType, OntologyGraph, OntologyObject, DatasetMapping, LinkTypeMapping, BreakingChangeInfo, EtLineage, Interface, BoundedContext, BcRelationship, BcSuggestion, BcInferenceResult } from './types'

// ── Projects ──────────────────────────────────────────────────────────────────
export const projectsApi = {
  list:   ()           => api.get<{ projects: Project[] }>('/api/projects').then(r => r.projects),
  get:    (id: string) => api.get<Project>(`/api/projects/${id}`),
  create: (name: string) => api.post<Project>('/api/projects', { name }),
  update: (id: string, name: string) => api.patch<Project>(`/api/projects/${id}`, { name }),
  delete: (id: string)   => api.delete<void>(`/api/projects/${id}`),
  folds:  (id: string)   => api.get<{ folds: Fold[] }>(`/api/projects/${id}/folds`).then(r => r.folds),
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
  promote:   (id: string, body: {
    entity_type_id?: string
    new_entity_type?: string
    sync_mode: string
    fold_id?: string
    primary_key_col?: string
    field_mapping?: Record<string, string>
    links?: { from_fk_col: string; to_entity_type_id: string; rel_type: string }[]
  }) => api.post<{ promoted: number; linked: number }>(`/api/datasets/${id}/promote`, body),
}

// ── Entity Types ──────────────────────────────────────────────────────────────
export const entityTypesApi = {
  list:   ()  => api.get<{ entity_types: EntityType[] }>('/api/ontology/schema').then(r => r.entity_types),
  create: (body: { name: string; display_name: string; color?: string; icon?: string; fold_id?: string; ddd_role?: string }) =>
    api.post<EntityType>('/api/ontology/schema', body),
  delete: (id: string) => api.delete<void>(`/api/ontology/schema/${id}`),
  addField: (etId: string, body: { name: string; data_type: string; is_required?: boolean; classification?: string }) =>
    api.post<void>(`/api/ontology/schema/${etId}/fields`, body),
  /** P0: check if deleting a field is breaking (dry run — POST with no strategy) */
  checkDeleteField: (fieldId: string) => api.post<BreakingChangeInfo>(`/api/ontology/fields/${fieldId}/safe-delete`, {}),
  /** P0: apply delete with strategy (always 'drop') */
  deleteFieldSafe: (fieldId: string) =>
    api.post<void>(`/api/ontology/fields/${fieldId}/safe-delete`, { strategy: 'drop' }),
  /** P0: check if changing field type is breaking */
  checkFieldTypeChange: (fieldId: string, newType: string) =>
    api.put<BreakingChangeInfo>(`/api/ontology/fields/${fieldId}/type`, { data_type: newType }),
  /** P0: apply field type change with strategy */
  applyFieldTypeChange: (fieldId: string, newType: string, strategy: string) =>
    api.put<void>(`/api/ontology/fields/${fieldId}/type`, { data_type: newType, strategy }),
  deleteField: (fieldId: string) => api.delete<void>(`/api/ontology/fields/${fieldId}`),
  /** P1a: change ET lifecycle status */
  setStatus: (etId: string, status: string) =>
    api.put<{ ok: boolean; status: string; affected_datasets: number }>(`/api/ontology/schema/${etId}/status`, { status }),
  /** P1b: data lineage */
  getLineage: (etId: string) => api.get<EtLineage>(`/api/ontology/schema/${etId}/lineage`),
  /** DDD role update */
  setDddRole: (etId: string, ddd_role: string) =>
    api.put<{ ok: boolean }>(`/api/ontology/schema/${etId}/ddd-role`, { ddd_role }),
  /** P2c: interfaces */
  listInterfaces: (etId: string) => api.get<{ interfaces: Interface[] }>(`/api/ontology/schema/${etId}/interfaces`).then(r => r.interfaces),
  addInterface: (etId: string, interfaceId: string) =>
    api.post<void>(`/api/ontology/schema/${etId}/interfaces`, { interface_id: interfaceId }),
  removeInterface: (etId: string, interfaceId: string) =>
    api.delete<void>(`/api/ontology/schema/${etId}/interfaces/${interfaceId}`),
}

// ── Interfaces ────────────────────────────────────────────────────────────────
export const interfacesApi = {
  list: () => api.get<{ interfaces: Interface[] }>('/api/interfaces').then(r => r.interfaces),
  create: (name: string, description?: string) =>
    api.post<Interface>('/api/interfaces', { name, description }),
  delete: (id: string) => api.delete<void>(`/api/interfaces/${id}`),
}

// ── Bounded Contexts ──────────────────────────────────────────────────────────
export const boundedContextsApi = {
  list: (foldId: string) =>
    api.get<{ bounded_contexts: BoundedContext[] }>(`/api/folds/${foldId}/bounded-contexts`).then(r => r.bounded_contexts),
  create: (foldId: string, name: string, color?: string) =>
    api.post<BoundedContext>(`/api/folds/${foldId}/bounded-contexts`, { name, color: color ?? '#6366f1' }),
  delete: (id: string) => api.delete<void>(`/api/bounded-contexts/${id}`),
  createRelationship: (body: { from_bc_id: string; to_bc_id: string; relationship_type: string; notes?: string }) =>
    api.post<BcRelationship>('/api/bc-relationships', body),
  deleteRelationship: (id: string) => api.delete<void>(`/api/bc-relationships/${id}`),
  sharedKernels: () => api.get<{ shared_kernels: Fold[] }>('/api/shared-kernels').then(r => r.shared_kernels),
  contextMap: (projectId: string) => api.get(`/api/projects/${projectId}/context-map`),
  /** P1: Union-Find dry-run — returns BC suggestions without writing */
  inferChildBcs: (foldId: string) => api.get<BcInferenceResult>(`/api/folds/${foldId}/bcs/infer`),
  /** P1: Write accepted suggestions to DB */
  applyBcSuggestions: (foldId: string, suggestions: BcSuggestion[]) =>
    api.post<{ created: BoundedContext[] }>(`/api/folds/${foldId}/bcs/apply-suggestions`, { suggestions }),
}

// ── Folds (extended) ──────────────────────────────────────────────────────────
export const foldsApi = {
  create: (projectId: string, name: string, description?: string, fold_type?: string) =>
    api.post<Fold>(`/api/projects/${projectId}/folds`, { name, description, fold_type }),
  delete: (id: string) => api.delete<void>(`/api/folds/${id}`),
}

// ── Sources ───────────────────────────────────────────────────────────────────
export const sourcesApi = {
  list:       ()           => api.get<{ sources: DataSource[] }>('/api/sources').then(r => r.sources),
  create:     (body: { name: string; source_type: string; config: Record<string, unknown> }) =>
    api.post<DataSource>('/api/sources', body),
  update:     (id: string, body: { name: string; source_type: string; config: Record<string, unknown> }) =>
    api.put<DataSource>(`/api/sources/${id}`, body),
  delete:     (id: string) => api.delete<void>(`/api/sources/${id}`),
  quickTest:  (body: { source_type: string; config: Record<string, unknown> }) =>
    api.post<{ status: string; files?: string[]; error?: string }>('/api/sources/quick-test', body),
  sync:       (id: string) => api.post<{ job_id: string }>(`/api/sources/${id}/sync`, {}),
  activate:   (id: string) => api.post<void>(`/api/sources/${id}/activate`, {}),
  deprecate:  (id: string) => api.post<void>(`/api/sources/${id}/deprecate`, {}),
  jobs:       (id: string) => api.get<{ jobs: SyncJob[] }>(`/api/sources/${id}/jobs`).then(r => r.jobs),
  datasets:   (id: string) => api.get<{ datasets: Dataset[] }>(`/api/sources/${id}/datasets`).then(r => r.datasets),
}

// ── Ontology Objects ──────────────────────────────────────────────────────────
export const objectsApi = {
  list:   (entityTypeId?: string) =>
    api.get<{ objects: OntologyObject[] }>(`/api/ontology/objects${entityTypeId ? `?entity_type_id=${entityTypeId}` : ''}`).then(r => r.objects),
  get:    (id: string) => api.get<OntologyObject>(`/api/ontology/objects/${id}`),
  create: (body: { entity_type_id: string; label: string; fields?: Record<string, unknown> }) =>
    api.post<OntologyObject>('/api/ontology/objects', body),
  delete: (id: string) => api.delete<void>(`/api/ontology/objects/${id}`),
}

// ── Ontology Links ─────────────────────────────────────────────────────────────
export const linksApi = {
  create: (body: { from_id: string; to_id: string; rel_type: string }) =>
    api.post<void>('/api/ontology/links', body),
  delete: (id: string) => api.delete<void>(`/api/ontology/links/${id}`),
}

// ── Graph ─────────────────────────────────────────────────────────────────────
export const graphApi = {
  get: (projectId?: string) =>
    api.get<OntologyGraph>(`/api/ontology/graph${projectId ? `?project_id=${projectId}` : ''}`),
  autoLink: () => api.post<{ ok: boolean; created: number; skipped: number }>('/api/ontology/auto-link', {}),
}
