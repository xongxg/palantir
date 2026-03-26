import { useState } from 'react'
import { toast } from 'sonner'
import { entityTypesApi } from '@/api'
import { useSchemaStore } from '@/store/schemaStore'


const FIELD_TYPES = ['string', 'number', 'boolean', 'date', 'datetime', 'reference']
const CLASSIFICATIONS = ['Internal', 'PII', 'Sensitive', 'Public']

interface Props { open: boolean; etId: string; onClose: () => void; onAdded: () => void }

export default function AddFieldDialog({ open, etId, onClose, onAdded }: Props) {
  const { entityTypes, upsertEntityType } = useSchemaStore()
  const [name,           setName]           = useState('')
  const [type,           setType]           = useState('string')
  const [classification, setClassification] = useState('Internal')
  const [required,       setRequired]       = useState(false)
  const [loading,        setLoading]        = useState(false)

  if (!open) return null

  async function handleSubmit() {
    if (!name.trim()) { toast.error('请输入字段名'); return }
    setLoading(true)
    try {
      await entityTypesApi.addField(etId, {
        name: name.trim(), data_type: type,
        classification, is_required: required,
      })
      // Refresh ET from store (optimistic: add field locally)
      const et = entityTypes.find(e => e.id === etId)
      if (et) {
        const newField = {
          id: `temp-${Date.now()}`, entity_type_id: etId,
          name: name.trim(), data_type: type,
          is_required: required, classification,
          sort_order: et.fields.length,
        }
        upsertEntityType({ ...et, fields: [...et.fields, newField] })
      }
      toast.success(`字段「${name}」已添加`)
      onAdded()
      onClose()
      setName(''); setType('string'); setClassification('Internal'); setRequired(false)
    } catch (e) {
      toast.error(String(e))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 w-80 space-y-4 shadow-2xl">
        <h3 className="text-base font-semibold text-slate-100">添加字段</h3>

        <div className="space-y-3">
          <label className="block">
            <span className="text-xs text-slate-400 mb-1 block">字段名</span>
            <input
              value={name} onChange={e => setName(e.target.value)}
              placeholder="amount"
              autoFocus
              onKeyDown={e => e.key === 'Enter' && handleSubmit()}
              className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500"
            />
          </label>

          <div className="flex gap-3">
            <label className="flex-1">
              <span className="text-xs text-slate-400 mb-1 block">数据类型</span>
              <select
                value={type} onChange={e => setType(e.target.value)}
                className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500"
              >
                {FIELD_TYPES.map(t => <option key={t} value={t}>{t}</option>)}
              </select>
            </label>
            <label className="flex-1">
              <span className="text-xs text-slate-400 mb-1 block">分类</span>
              <select
                value={classification} onChange={e => setClassification(e.target.value)}
                className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500"
              >
                {CLASSIFICATIONS.map(c => <option key={c} value={c}>{c}</option>)}
              </select>
            </label>
          </div>

          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox" checked={required}
              onChange={e => setRequired(e.target.checked)}
              className="accent-indigo-500"
            />
            <span className="text-sm text-slate-400">必填字段</span>
          </label>
        </div>

        <div className="flex gap-2 justify-end pt-1">
          <button onClick={onClose} className="px-4 py-2 text-sm text-slate-400 hover:text-slate-200 border border-slate-700 rounded-lg transition-colors">
            取消
          </button>
          <button
            onClick={handleSubmit} disabled={loading}
            className="px-4 py-2 text-sm bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white rounded-lg transition-colors"
          >
            {loading ? '添加中…' : '添加'}
          </button>
        </div>
      </div>
    </div>
  )
}
