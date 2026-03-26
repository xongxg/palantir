import { useState } from 'react'
import { toast } from 'sonner'
import { entityTypesApi } from '@/api'
import { useSchemaStore } from '@/store/schemaStore'

const DDD_ROLES = ['entity', 'aggregate_root', 'value_object', 'domain_event', 'service']

interface Props { open: boolean; onClose: () => void }

export default function CreateETDialog({ open, onClose }: Props) {
  const { upsertEntityType, select } = useSchemaStore()
  const [name,    setName]    = useState('')
  const [display, setDisplay] = useState('')
  const [color,   setColor]   = useState('#6366f1')
  const [icon,    setIcon]    = useState('●')
  const [role,    setRole]    = useState('entity')
  const [loading, setLoading] = useState(false)

  if (!open) return null

  async function handleSubmit() {
    if (!name.trim()) { toast.error('请输入标识名'); return }
    setLoading(true)
    try {
      const et = await entityTypesApi.create({
        name: name.trim(),
        display_name: display.trim() || name.trim(),
        color, icon, ddd_role: role,
      })
      upsertEntityType(et)
      select(et.id)
      toast.success(`Entity Type「${et.display_name}」已创建`)
      onClose()
      setName(''); setDisplay(''); setColor('#6366f1'); setIcon('●'); setRole('entity')
    } catch (e) {
      toast.error(String(e))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 w-96 space-y-4 shadow-2xl">
        <h3 className="text-base font-semibold text-slate-100">新建实体类型</h3>

        <div className="space-y-3">
          <label className="block">
            <span className="text-xs text-slate-400 mb-1 block">标识名 <span className="text-slate-600">（英文，无空格）</span></span>
            <input
              value={name}
              onChange={e => setName(e.target.value)}
              placeholder="contract"
              className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500"
            />
          </label>

          <label className="block">
            <span className="text-xs text-slate-400 mb-1 block">显示名称</span>
            <input
              value={display}
              onChange={e => setDisplay(e.target.value)}
              placeholder="合同"
              className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500"
            />
          </label>

          <div className="flex gap-3">
            <label className="flex-1">
              <span className="text-xs text-slate-400 mb-1 block">颜色</span>
              <input
                type="color" value={color}
                onChange={e => setColor(e.target.value)}
                className="w-full h-9 rounded-lg border border-slate-700 bg-slate-950 px-1 py-1 cursor-pointer"
              />
            </label>
            <label className="flex-1">
              <span className="text-xs text-slate-400 mb-1 block">图标</span>
              <input
                value={icon}
                onChange={e => setIcon(e.target.value)}
                placeholder="●"
                className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500"
              />
            </label>
          </div>

          <label className="block">
            <span className="text-xs text-slate-400 mb-1 block">DDD 角色</span>
            <select
              value={role} onChange={e => setRole(e.target.value)}
              className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500"
            >
              {DDD_ROLES.map(r => <option key={r} value={r}>{r}</option>)}
            </select>
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
            {loading ? '创建中…' : '创建'}
          </button>
        </div>
      </div>
    </div>
  )
}
