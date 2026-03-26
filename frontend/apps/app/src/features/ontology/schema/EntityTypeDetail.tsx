import { useState } from 'react'
import { toast } from 'sonner'
import { useSchemaStore } from '@/store/schemaStore'
import { entityTypesApi } from '@/api'
import AddFieldDialog from './AddFieldDialog'
import { cn } from '@/lib/utils'

const CLASSIFICATION_COLORS: Record<string, string> = {
  PII:       'text-red-400 bg-red-950/40',
  Sensitive: 'text-amber-400 bg-amber-950/40',
  Public:    'text-green-400 bg-green-950/40',
  Internal:  'text-slate-400 bg-slate-800/40',
}

export default function EntityTypeDetail() {
  const { entityTypes, selectedId, removeEntityType, upsertEntityType } = useSchemaStore()
  const [showAddField, setShowAddField] = useState(false)
  const [deleting, setDeleting] = useState(false)

  const et = entityTypes.find(e => e.id === selectedId)

  if (!et) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-center">
        <div className="text-5xl mb-3 text-slate-800">🗂</div>
        <p className="text-sm text-slate-400">选择实体类型管理其字段</p>
        <p className="text-xs text-slate-600 mt-1.5">或点击左侧「+ 新建」</p>
      </div>
    )
  }

  async function handleDeleteET() {
    if (!confirm(`确认删除「${et!.display_name || et!.name}」？此操作不可撤销。`)) return
    setDeleting(true)
    try {
      await entityTypesApi.delete(et!.id)
      removeEntityType(et!.id)
      toast.success('已删除')
    } catch (e) {
      toast.error(String(e))
    } finally {
      setDeleting(false)
    }
  }

  async function handleDeleteField(fieldId: string, fieldName: string) {
    if (!confirm(`确认删除字段「${fieldName}」？`)) return
    try {
      await entityTypesApi.deleteField(fieldId)
      upsertEntityType({ ...et!, fields: et!.fields.filter(f => f.id !== fieldId) })
      toast.success('字段已删除')
    } catch (e) {
      toast.error(String(e))
    }
  }

  async function refreshET() {
    try {
      const ets = await entityTypesApi.list()
      const updated = ets.find(e => e.id === et!.id)
      if (updated) upsertEntityType(updated)
    } catch (_) {}
  }

  return (
    <div className="p-6 max-w-3xl space-y-5">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <span className="text-4xl" style={{ color: et.color }}>{et.icon || '●'}</span>
          <div>
            <h2 className="text-xl font-bold text-white">{et.display_name || et.name}</h2>
            <p className="text-xs text-slate-500 font-mono mt-0.5">
              {et.name}
              {et.ddd_role && et.ddd_role !== 'entity' && (
                <span className="ml-2 text-slate-600">· {et.ddd_role}</span>
              )}
              {et.namespace && (
                <span className="ml-2 text-slate-600">· {et.namespace}</span>
              )}
            </p>
          </div>
        </div>
        <div className="flex gap-2">
          <button
            onClick={() => setShowAddField(true)}
            className="px-3 py-1.5 text-xs bg-indigo-600 hover:bg-indigo-500 text-white rounded-lg transition-colors"
          >
            + 添加字段
          </button>
          <button
            onClick={handleDeleteET}
            disabled={deleting}
            className="px-3 py-1.5 text-xs bg-red-900/40 hover:bg-red-800/60 text-red-400 border border-red-800/40 rounded-lg transition-colors disabled:opacity-50"
          >
            {deleting ? '删除中…' : '删除'}
          </button>
        </div>
      </div>

      {/* Fields table */}
      <div className="bg-slate-900 border border-slate-800 rounded-lg overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="text-[11px] text-slate-500 border-b border-slate-800 bg-slate-900/80">
              {['字段名', '类型', '分类', '必填', ''].map((h, i) => (
                <th key={i} className={cn('text-left px-4 py-2.5 font-medium', i === 4 && 'w-8')}>{h}</th>
              ))}
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-800/60">
            {et.fields.map(f => (
              <tr key={f.id} className="hover:bg-slate-800/30 transition-colors">
                <td className="px-4 py-2.5 text-slate-200 font-mono text-xs">{f.name}</td>
                <td className="px-4 py-2.5">
                  <span className="text-xs font-mono text-slate-400">{f.data_type}</span>
                </td>
                <td className="px-4 py-2.5">
                  <span className={cn(
                    'text-[10px] px-1.5 py-0.5 rounded font-medium',
                    CLASSIFICATION_COLORS[f.classification] ?? CLASSIFICATION_COLORS.Internal,
                  )}>
                    {f.classification}
                  </span>
                </td>
                <td className="px-4 py-2.5">
                  {f.is_required
                    ? <span className="text-xs text-amber-400">必填</span>
                    : <span className="text-xs text-slate-700">—</span>}
                </td>
                <td className="px-4 py-2.5">
                  <button
                    onClick={() => handleDeleteField(f.id, f.name)}
                    className="text-slate-700 hover:text-red-400 transition-colors text-xs"
                  >✕</button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {et.fields.length === 0 && (
          <div className="px-4 py-8 text-center text-slate-600 text-sm">
            暂无字段 — 在上方添加
          </div>
        )}
      </div>

      <AddFieldDialog
        open={showAddField}
        etId={et.id}
        onClose={() => setShowAddField(false)}
        onAdded={refreshET}
      />
    </div>
  )
}
