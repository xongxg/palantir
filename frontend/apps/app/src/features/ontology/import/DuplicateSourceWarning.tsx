import { useImportStore } from '@/store/importStore'
import { classifySourceRelation } from './lib'

interface Props {
  selectedEtId: string
  currentDatasetId: string
}

export default function DuplicateSourceWarning({ selectedEtId, currentDatasetId }: Props) {
  const { datasets, entityTypes, columns, syncMode, setSyncMode } = useImportStore()

  if (!selectedEtId) return null

  const others = datasets.filter(d => d.entity_type_id === selectedEtId && d.id !== currentDatasetId)
  if (!others.length) return null

  const et = entityTypes.find(e => e.id === selectedEtId)
  const etName = et?.display_name || et?.name || selectedEtId
  const etFields = et?.fields ?? []

  const relation = classifySourceRelation(columns, etFields)
  const names = others.map(d => `「${d.name || d.id}」`).join('、')

  let icon: string, badge: string, hint: string
  if (relation === 'duplicate') {
    icon = '⚠️'; badge = '重复源'
    hint = `当前数据集字段与 ET 完全重合（可能是主备/迁移场景）。Upsert 按主键合并可安全共存，不会产生重复行。`
  } else if (relation === 'enrichment') {
    const newFields = columns
      .filter(c => !c.ignored && !etFields.find(f => f.name === (c.editedName || c.name)))
      .map(c => c.editedName || c.name)
    icon = 'ℹ️'; badge = '增强源'
    hint = `检测到 ${newFields.length} 个新字段（${newFields.slice(0, 3).join('、')}${newFields.length > 3 ? `…` : ''}），当前数据集将补充新字段到 ET。Upsert 按主键合并，新字段写入已有对象。`
  } else {
    icon = 'ℹ️'; badge = '互补源'
    hint = `当前数据集字段与已有源完全不重叠，是互补数据。Upsert 模式可安全合并，两份数据在同一 ET 下共存。`
  }

  // Auto-switch snapshot → upsert
  if (syncMode === 'snapshot') {
    // Use setTimeout to avoid calling setState during render
    setTimeout(() => setSyncMode('upsert'), 0)
  }

  return (
    <div className="mt-2 px-3 py-2 rounded-lg border-l-2 border-amber-500 bg-amber-950/40 text-xs text-amber-200 leading-relaxed">
      <span className="mr-1">{icon}</span>
      <span className="px-1.5 py-0.5 rounded text-[9px] bg-amber-900 text-amber-300 mr-1.5">{badge}</span>
      ET「{etName}」已有 {others.length} 个数据源：{names}。{hint}
    </div>
  )
}
