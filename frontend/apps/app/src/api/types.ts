// ── Projects ──────────────────────────────────────────────────────────────────
export interface Project {
  id: string
  name: string
  created_at: string
  updated_at: string
}

// ── Folds ─────────────────────────────────────────────────────────────────────
export interface Fold {
  id: string
  project_id: string
  name: string
  description?: string
  fold_type: 'normal' | 'shared_kernel'
  created_at: string
}

// ── Data Sources ──────────────────────────────────────────────────────────────
export interface DataSource {
  id: string
  fold_id?: string
  name: string
  source_type: string
  status: string
  sync_mode?: string
  deprecated?: boolean
  record_count?: number
  last_sync_at?: string
  config?: Record<string, unknown>
  created_at: string
}

export interface SyncJob {
  id: string
  source_id: string
  status: 'pending' | 'running' | 'completed' | 'failed'
  total_records?: number
  error?: string
  started_at?: string
  finished_at?: string
}

// ── Datasets ──────────────────────────────────────────────────────────────────
export interface Dataset {
  id: string
  source_id: string
  source_name?: string
  name: string
  entity_type_id?: string
  current_version: number
  record_count: number
  fold_id?: string
  fold_name?: string
  created_at: string
}

// ── Entity Types ──────────────────────────────────────────────────────────────
export interface EntityField {
  id: string
  entity_type_id: string
  name: string
  data_type: string
  is_required: boolean
  classification: string
  sort_order: number
}

export interface EntityType {
  id: string
  name: string
  display_name: string
  color: string
  icon: string
  fold_id?: string
  namespace?: string
  ddd_role: string
  bc_id?: string
  /** 'draft' | 'active' | 'deprecated' */
  status: string
  fields: EntityField[]
  created_at: string
}

// ── Breaking change ─────────────────────────────────────────────────────────
export interface BreakingChangeInfo {
  breaking: boolean
  affected_count: number
  change_type: string
  old_value?: string
  new_value?: string
  strategies: string[]
}

// ── Lineage ─────────────────────────────────────────────────────────────────
export interface LineageSource {
  dataset_id: string
  dataset_name: string
  source_id: string
  source_name: string
  source_type: string
  fold_id: string
  fold_name: string
  primary_key_col: string
  sync_mode: string
  last_synced_at?: string
  record_count: number
}

export interface EtLineage {
  entity_type_id: string
  sources: LineageSource[]
  total_records: number
}

// ── Interfaces ──────────────────────────────────────────────────────────────
export interface InterfaceField {
  field_name: string
  field_type: string
  required: boolean
  description?: string
}

export interface Interface {
  id: string
  name: string
  description?: string
  is_builtin: boolean
  created_at: string
  fields: InterfaceField[]
}

// ── Ontology Objects ──────────────────────────────────────────────────────────
export interface OntologyLink {
  id: string
  from_id: string
  to_id: string
  rel_type: string
  other_id?: string
  other_label?: string
  other_et_name?: string
  other_et_id?: string
}

export interface OntologyObject {
  id: string
  entity_type_id: string
  entity_type_name: string
  external_id?: string
  label: string
  fields: Record<string, unknown>
  links?: OntologyLink[]
  created_at: string
  updated_at: string
}

// ── Bounded Contexts ──────────────────────────────────────────────────────────
export interface BoundedContext {
  id: string
  fold_id: string
  name: string
  color: string
  auto_detected: boolean
  created_at: string
}

/** One BC suggestion returned by the Union-Find inference */
export interface BcSuggestion {
  suggested_name: string
  color: string
  et_ids: string[]
  entity_types: { id: string; name: string; display_name: string; ddd_role: string }[]
  aggregate_root_id?: string
  confidence: number
  cross_bc_links: { from_et_id: string; to_et_id: string }[]
}

export interface BcInferenceResult {
  suggestions: BcSuggestion[]
}

export interface BcRelationship {
  id: string
  from_bc_id: string
  from_name: string
  to_bc_id: string
  to_name: string
  relationship_type: 'shared_kernel' | 'customer_supplier' | 'conformist' | 'acl' | 'open_host'
  notes?: string
  created_at: string
}

// ── Mappings ──────────────────────────────────────────────────────────────────
export interface DatasetMapping {
  id: string
  dataset_id: string
  entity_type_id: string
  primary_key_col: string
  field_mapping: Record<string, string>
  sync_mode: string
  created_at: string
  updated_at: string
}

export interface LinkTypeMapping {
  id: string
  dataset_id: string
  from_fk_col: string
  to_entity_type_id: string
  rel_type: string
}

// ── Graph ─────────────────────────────────────────────────────────────────────
export interface GraphNode {
  id: string
  label: string
  entity_type: string
  et_id?: string
  color?: string
  fold_id?: string
}

export interface GraphEdge {
  id: string
  source: string
  target: string
  rel_type: string
}

export interface OntologyGraph {
  nodes: GraphNode[]
  edges: GraphEdge[]
  entity_types?: EntityType[]
}

export interface ActionTypeParam {
  name: string
  type: string          // string | number | boolean | date
  required: boolean
  description?: string
}

export interface ActionType {
  id: string
  name: string
  display_name: string
  target_et_id?: string
  level: 'object' | 'bc' | 'fold' | 'app'
  from_states: string[]
  to_state?: string
  params: ActionTypeParam[]
  trigger: 'manual' | 'event' | 'cron'
  allowed_personas: string[]
  bc_id?: string
  saga_def_id?: string
  status: 'draft' | 'active' | 'deprecated'
  created_at: string
}
