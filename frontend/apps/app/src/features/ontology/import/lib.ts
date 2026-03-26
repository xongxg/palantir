import type { ColumnDef, LinkRow } from '@/store/importStore'
import type { EntityType } from '@/api/types'

/** Infer column type from sample values */
export function inferType(samples: string[]): string {
  const vals = samples.filter(s => s !== '')
  if (!vals.length) return 'string'
  if (vals.every(s => /^-?\d+(\.\d+)?$/.test(s))) return 'number'
  if (vals.every(s => /^\d{4}-\d{2}-\d{2}/.test(s))) return 'date'
  if (vals.every(s => s === 'true' || s === 'false' || s === '0' || s === '1')) return 'boolean'
  return 'string'
}

/** Guess primary key column name */
export function guessPk(columns: ColumnDef[]): string {
  const names = columns.map(c => c.name.toLowerCase())
  for (const cand of ['id', '_id', 'uuid', 'key', 'pk']) {
    const idx = names.indexOf(cand)
    if (idx >= 0) return columns[idx].name
  }
  const col = columns.find(c =>
    c.name.toLowerCase().endsWith('_id') || c.name.toLowerCase() === 'id',
  )
  return col?.name ?? columns[0]?.name ?? ''
}

/** Auto-infer FK relationships from _id suffix columns */
export function autoInferLinks(
  columns: ColumnDef[],
  entityTypes: EntityType[],
  selfEtId: string,
): LinkRow[] {
  const links: LinkRow[] = []
  for (const col of columns) {
    if (col.ignored) continue
    const name = col.name.toLowerCase()
    if (!name.endsWith('_id') || name === 'id') continue
    const stem = name.slice(0, -3) // strip _id
    const match = entityTypes.find(et => {
      if (et.id === selfEtId) return false
      const ename = (et.name || '').toLowerCase().replace(/[_\s-]+/g, '')
      const dname = (et.display_name || '').toLowerCase().replace(/[_\s-]+/g, '')
      return ename === stem || dname === stem || ename.startsWith(stem) || stem.startsWith(ename)
    })
    if (match) links.push({ fkCol: col.name, toEntityTypeId: match.id, relType: 'HAS' })
  }
  return links
}

/** Compare field sets to classify multi-source relationship */
export type SourceRelation = 'duplicate' | 'enrichment' | 'complementary'

export function classifySourceRelation(
  currentCols: ColumnDef[],
  etFields: EntityType['fields'],
): SourceRelation {
  if (!etFields.length || !currentCols.length) return 'duplicate'
  const etNames = new Set(etFields.map(f => f.name))
  const currNames = currentCols.filter(c => !c.ignored).map(c => c.editedName || c.name)
  const newFields = currNames.filter(n => !etNames.has(n))
  const overlap   = currNames.filter(n => etNames.has(n))
  if (newFields.length === 0) return 'duplicate'
  if (overlap.length > 0)     return 'enrichment'
  return 'complementary'
}

export const SYNC_MODE_TIPS: Record<string, string> = {
  snapshot: '⚡ 每次 Sync 先清空本 Dataset 的所有 Ontology 对象，再全量写入。适合：每次推送完整状态快照（最常见）。误用风险：若数据是增量，历史记录会被删除。',
  append:   '📥 每次 Sync 追加新行，历史行保留。适合：日志、事件流等只增不改的数据。风险：若上游数据有修正，旧错误行不会被覆盖。',
  upsert:   '🔄 按主键（primary_key_col）合并：有则更新，无则插入。适合：状态同步、主数据更新（员工信息、产品）。⚠ 必须配置主键列，否则降级为 Append。',
}
