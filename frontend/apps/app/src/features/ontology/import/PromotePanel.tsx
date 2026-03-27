import { useState, useEffect } from 'react'
import { toast } from 'sonner'
import { useImportStore } from '@/store/importStore'
import { datasetsApi, entityTypesApi } from '@/api'
import { inferType, guessPk, autoInferLinks, SYNC_MODE_TIPS } from './lib'
import SchemaTable from './SchemaTable'
import DuplicateSourceWarning from './DuplicateSourceWarning'
import RelationshipSection from './RelationshipSection'
import { cn } from '@/lib/utils'
import type { Dataset } from '@/api/types'

interface Props { dataset: Dataset }

export default function PromotePanel({ dataset }: Props) {
  const {
    columns, entityTypes, syncMode, links,
    promoting, promoteResult,
    setColumns, setSyncMode, setLinks, setSavedMapping,
    setPromoting, setPromoteResult, markDatasetPromoted,
  } = useImportStore()

  const [selectedEtId, setSelectedEtId] = useState(dataset.entity_type_id ?? '')
  const [newTypeName,  setNewTypeName]  = useState('')
  const [pk, setPk] = useState('')

  const isPromoted = dataset.entity_type_id != null

  // Load dataset columns + saved mapping on mount
  useEffect(() => {
    let cancelled = false
    async function load() {
      try {
        // Load columns from records
        const { records } = await datasetsApi.records(dataset.id, 3)
        if (cancelled) return
        if (records.length) {
          const first = records[0] as Record<string, unknown>
          const cols = Object.keys(first).map(name => ({
            name,
            editedName: name,
            inferredType: inferType(
              records.map(r => String((r as Record<string, unknown>)[name] ?? '')).filter(Boolean),
            ),
            samples: records
              .map(r => String((r as Record<string, unknown>)[name] ?? ''))
              .filter(Boolean)
              .slice(0, 2),
            ignored: false,
            isPk: false,
          }))
          setColumns(cols)

          // Load saved mapping
          const { mapping } = await datasetsApi.getMapping(dataset.id)
          if (cancelled) return
          if (mapping) {
            const fm = typeof mapping.field_mapping === 'string'
              ? JSON.parse(mapping.field_mapping as unknown as string)
              : (mapping.field_mapping ?? {})
            setSavedMapping(mapping.primary_key_col, fm)
            setPk(mapping.primary_key_col)
            setSyncMode(mapping.sync_mode)
            if (mapping.entity_type_id) setSelectedEtId(mapping.entity_type_id)

            // Apply saved field mapping to columns
            const hasSaved = Object.keys(fm).length > 0
            setColumns(cols.map(col => {
              const mapped = fm[col.name]
              if (mapped !== undefined) return { ...col, editedName: mapped }
              if (hasSaved) return { ...col, ignored: true }
              return col
            }))
          } else {
            const guessed = guessPk(cols)
            setPk(guessed)
          }

          // Auto-infer links
          const etId = mapping?.entity_type_id ?? dataset.entity_type_id ?? ''
          const inferred = autoInferLinks(cols, entityTypes, etId)
          setLinks(inferred)
        }
      } catch (e) {
        console.error(e)
      }
    }
    load()
    return () => { cancelled = true }
  }, [dataset.id])

  // Auto-match ET by dataset name if nothing selected
  useEffect(() => {
    if (selectedEtId || !entityTypes.length) return
    const stem = (dataset.name || '').replace(/\.[^.]+$/, '').toLowerCase().replace(/[_\s-]+/g, '')
    const match = entityTypes.find(et => {
      const ename = (et.name || '').toLowerCase().replace(/[_\s-]+/g, '')
      const dname = (et.display_name || '').toLowerCase().replace(/[_\s-]+/g, '')
      return ename === stem || dname === stem || stem.startsWith(ename) || ename.startsWith(stem)
    })
    if (match) setSelectedEtId(match.id)
  }, [entityTypes, dataset.name])

  async function handleSaveET() {
    if (!newTypeName.trim() && !selectedEtId) return
    try {
      let resolvedId = selectedEtId
      if (newTypeName.trim()) {
        try {
          const et = await entityTypesApi.create({ name: newTypeName, display_name: newTypeName })
          resolvedId = et.id
          setSelectedEtId(et.id)
          // Persist schema fields
          await Promise.allSettled(
            columns
              .filter(c => !c.ignored)
              .map(c => entityTypesApi.addField(et.id, {
                name: c.editedName || c.name,
                data_type: c.inferredType || 'string',
              }))
          )
          toast.success(`Entity Type「${et.display_name}」已创建`)
        } catch {
          // Already exists — find and reuse
          const existing = entityTypes.find(t =>
            t.display_name.toLowerCase() === newTypeName.trim().toLowerCase() ||
            t.name.toLowerCase() === newTypeName.trim().toLowerCase().replace(/\s+/g, '_')
          )
          if (existing) {
            resolvedId = existing.id
            setSelectedEtId(existing.id)
            toast.info(`使用已有 Entity Type「${existing.display_name}」`)
          } else {
            throw new Error(`Entity Type「${newTypeName}」已存在，请从下拉菜单选择`)
          }
        }
      }
      // Save mapping
      const fieldMapping: Record<string, string> = {}
      columns.forEach(c => { if (!c.ignored) fieldMapping[c.name] = c.editedName || c.name })
      await datasetsApi.saveMapping(dataset.id, {
        entity_type_id: resolvedId || undefined,
        primary_key_col: pk,
        field_mapping: fieldMapping,
        sync_mode: syncMode,
      })
      toast.success('映射已保存')
    } catch (e) {
      toast.error(String(e))
    }
  }

  async function handlePromote() {
    let etId   = selectedEtId
    const etName = newTypeName.trim()
    if (!etId && !etName) {
      toast.error('请选择或输入 Entity Type')
      return
    }
    setPromoting(true)
    setPromoteResult(null)
    try {
      // Ensure ET exists and fields are saved before promoting
      if (etName && !etId) {
        try {
          const et = await entityTypesApi.create({ name: etName, display_name: etName })
          etId = et.id
          setSelectedEtId(et.id)
          await Promise.allSettled(
            columns.filter(c => !c.ignored).map(c =>
              entityTypesApi.addField(et.id, { name: c.editedName || c.name, data_type: c.inferredType || 'string' })
            )
          )
        } catch {
          const existing = entityTypes.find(t =>
            t.display_name.toLowerCase() === etName.toLowerCase() ||
            t.name.toLowerCase() === etName.toLowerCase().replace(/\s+/g, '_')
          )
          if (existing) { etId = existing.id; setSelectedEtId(existing.id) }
          else throw new Error(`无法创建 Entity Type「${etName}」`)
        }
      }
      const fieldMapping: Record<string, string> = {}
      columns.forEach(c => { if (!c.ignored) fieldMapping[c.name] = c.editedName || c.name })
      const result = await datasetsApi.promote(dataset.id, {
        entity_type_id: etId || undefined,
        new_entity_type: undefined,
        sync_mode: syncMode,
        primary_key_col: pk || undefined,
        field_mapping: Object.keys(fieldMapping).length ? fieldMapping : undefined,
        links: links.map(l => ({ from_fk_col: l.fkCol, to_entity_type_id: l.toEntityTypeId, rel_type: l.relType })),
      })
      markDatasetPromoted(dataset.id, etId)
      setPromoteResult({ ok: true, message: `✓ Promoted ${result.promoted} objects, ${result.linked} links` })
      toast.success(`Promote 成功：${result.promoted} 条记录`)
    } catch (e) {
      setPromoteResult({ ok: false, message: String(e) })
      toast.error(String(e))
    } finally {
      setPromoting(false)
    }
  }

  const selectedET = entityTypes.find(e => e.id === selectedEtId)

  return (
    <div className="p-6 max-w-3xl w-full">
      {/* Header */}
      <div className="flex items-start justify-between mb-5">
        <div>
          <h2 className="text-lg font-bold text-white">{dataset.name || dataset.id}</h2>
          <p className="text-xs text-slate-500 mt-0.5">
            {dataset.record_count ?? 0} records · source: {dataset.source_id}
          </p>
        </div>
        <span className={cn(
          'text-xs px-2 py-1 rounded-full font-medium',
          isPromoted
            ? 'bg-emerald-900/40 text-emerald-300 border border-emerald-800/50'
            : 'bg-amber-900/40 text-amber-300 border border-amber-800/50',
        )}>
          {isPromoted ? '✓ Promoted' : 'Not promoted'}
        </span>
      </div>

      {/* Section 1: Schema */}
      <div className="bg-slate-900 border border-slate-800 rounded-lg p-4 mb-4">
        <div className="flex items-center justify-between mb-1">
          <p className="text-[11px] font-semibold text-slate-400 uppercase tracking-wider">1 · Schema 发现</p>
          <button
            onClick={() => {
              const guessed = guessPk(columns)
              setPk(guessed)
              setColumns(columns.map(c => ({ ...c, editedName: c.name, inferredType: inferType(c.samples), ignored: false })))
            }}
            className="text-xs text-slate-500 hover:text-slate-300 transition-colors"
          >
            ↺ 重置
          </button>
        </div>
        <p className="text-[11px] text-slate-600 mb-3">
          系统自动识别列名和类型。确认后保存为 Entity Type，下次相同结构的 CSV 可直接复用。
        </p>

        <table className="w-full text-xs border-collapse">
          <thead>
            <tr className="border-b border-slate-800">
              {['列名', '类型', '样例值', '主键', '忽略'].map((h, i) => (
                <th key={h} className={cn(
                  'py-1.5 px-2 text-slate-500 font-medium text-left',
                  i >= 3 && 'text-center w-10',
                )}>
                  {h}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            <SchemaTable savedPk={pk} onPkChange={setPk} />
          </tbody>
        </table>

        {/* ET selection */}
        <div className="mt-4 pt-3 border-t border-slate-800">
          <p className="text-[11px] text-slate-500 mb-2">确认 Schema 后保存为 Entity Type，供后续复用：</p>
          <div className="flex gap-2 items-center">
            <input
              value={newTypeName}
              onChange={e => { setNewTypeName(e.target.value); if (e.target.value) setSelectedEtId('') }}
              placeholder="输入 Entity Type 名称，如 Flight"
              className="flex-1 bg-slate-950 border border-slate-700 rounded px-3 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
            />
            <span className="text-[11px] text-slate-600 flex-shrink-0">或选已有</span>
            <select
              value={selectedEtId}
              onChange={e => { setSelectedEtId(e.target.value); if (e.target.value) setNewTypeName('') }}
              className="flex-1 bg-slate-950 border border-slate-700 rounded px-3 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
            >
              <option value="">— 已有类型 —</option>
              {entityTypes.filter(et => et.status !== 'deprecated').map(et => (
                <option key={et.id} value={et.id} disabled={et.status === 'draft'}>
                  {et.status === 'draft' ? '[草稿] ' : ''}{et.namespace ? `[${et.namespace}] ` : ''}{et.display_name || et.name}
                </option>
              ))}
            </select>
            <button onClick={handleSaveET} className="flex-shrink-0 px-3 py-1.5 text-xs bg-slate-800 border border-slate-700 rounded hover:bg-slate-700 transition-colors text-slate-300">
              保存为 Entity Type
            </button>
          </div>

          {/* Sync mode */}
          <div className="flex items-center gap-2 mt-2">
            <span className="text-[11px] text-slate-600 flex-shrink-0">同步模式：</span>
            <select
              value={syncMode}
              onChange={e => setSyncMode(e.target.value)}
              className="flex-1 bg-slate-950 border border-slate-700 rounded px-3 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
            >
              <option value="snapshot">Snapshot — 全量替换（每次清空重写）</option>
              <option value="append">Append — 追加（历史行保留，新行累加）</option>
              <option value="upsert">Upsert — 按主键合并（修改已有行）</option>
            </select>
          </div>

          {/* Sync mode tip */}
          {SYNC_MODE_TIPS[syncMode] && (
            <p className="mt-1.5 text-[11px] text-slate-500 px-2 py-1.5 rounded bg-slate-950 border-l-2 border-slate-700 leading-relaxed">
              {SYNC_MODE_TIPS[syncMode]}
            </p>
          )}

          {/* Multi-source warning */}
          <DuplicateSourceWarning selectedEtId={selectedEtId} currentDatasetId={dataset.id} />

          {/* Auto-match hint */}
          {selectedET && !dataset.entity_type_id && (
            <p className="text-[11px] text-slate-500 mt-1.5">
              ↑ 根据名称自动匹配「{selectedET.display_name || selectedET.name}」，确认后点 Promote。
            </p>
          )}
        </div>
      </div>

      {/* Section 2: Relationships */}
      <div className="bg-slate-900 border border-slate-800 rounded-lg p-4 mb-4">
        <div className="flex items-center justify-between mb-1">
          <p className="text-[11px] font-semibold text-slate-400 uppercase tracking-wider">
            2 · 关联关系 <span className="text-slate-600 normal-case font-normal">(可选)</span>
          </p>
          <div className="flex gap-2">
            <button
              onClick={() => setLinks(autoInferLinks(columns, entityTypes, selectedEtId))}
              className="text-xs text-slate-500 hover:text-slate-300 transition-colors"
            >↺ 自动推导</button>
          </div>
        </div>
        <p className="text-[11px] text-slate-600 mb-3">
          自动识别 <code className="text-slate-400">_id</code> 后缀列推导关联，可修改目标类型或删除。
        </p>
        <RelationshipSection entityTypeId={selectedEtId} />
      </div>

      {/* Result */}
      {promoteResult && (
        <div className={cn(
          'mt-3 px-4 py-2.5 rounded-lg text-sm',
          promoteResult.ok
            ? 'bg-emerald-900/30 text-emerald-300 border border-emerald-800/40'
            : 'bg-red-900/30 text-red-300 border border-red-800/40',
        )}>
          {promoteResult.message}
        </div>
      )}
    </div>
  )
}
