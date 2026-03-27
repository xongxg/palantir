import { useState, useEffect } from 'react'
import { toast } from 'sonner'
import { actionTypesApi } from '@/api'
import type { ActionType, ActionTypeParam } from '@/api/types'
import { cn } from '@/lib/utils'

const LEVEL_LABEL: Record<string, string> = {
  object: '对象级', bc: 'BC 级', fold: 'Fold 级', app: '应用级（Saga）',
}
const TRIGGER_LABEL: Record<string, string> = {
  manual: '手动', event: '事件触发', cron: '定时',
}
const STATUS_CLS: Record<string, string> = {
  draft:      'text-slate-400 bg-slate-800',
  active:     'text-green-400 bg-green-900/40',
  deprecated: 'text-amber-400 bg-amber-900/40',
}
const STATUS_LABEL: Record<string, string> = {
  draft: '草稿', active: '活跃', deprecated: '已废弃',
}

// ── 空白表单 ──────────────────────────────────────────────────────────────────
function emptyForm(): Partial<ActionType> {
  return {
    name: '', display_name: '', level: 'object', trigger: 'manual',
    from_states: [], to_state: '', params: [], allowed_personas: [],
  }
}

// ── ActionType 编辑弹窗 ───────────────────────────────────────────────────────
interface EditDialogProps {
  etId: string
  initial: Partial<ActionType>
  onClose: () => void
  onSaved: () => void
}

function EditDialog({ etId, initial, onClose, onSaved }: EditDialogProps) {
  const isNew = !initial.id
  const [form, setForm] = useState<Partial<ActionType>>({ ...emptyForm(), ...initial })
  const [saving, setSaving] = useState(false)

  // helpers for array fields
  const setFromStates = (v: string) =>
    setForm(f => ({ ...f, from_states: v.split(',').map(s => s.trim()).filter(Boolean) }))
  const setPersonas = (v: string) =>
    setForm(f => ({ ...f, allowed_personas: v.split(',').map(s => s.trim()).filter(Boolean) }))

  // params editor
  function addParam() {
    setForm(f => ({ ...f, params: [...(f.params ?? []), { name: '', type: 'string', required: false }] }))
  }
  function updateParam(i: number, p: Partial<ActionTypeParam>) {
    setForm(f => {
      const params = [...(f.params ?? [])]
      params[i] = { ...params[i], ...p }
      return { ...f, params }
    })
  }
  function removeParam(i: number) {
    setForm(f => ({ ...f, params: (f.params ?? []).filter((_, idx) => idx !== i) }))
  }

  async function save() {
    if (!form.name?.trim() || !form.display_name?.trim()) {
      toast.error('名称和显示名称不能为空')
      return
    }
    setSaving(true)
    try {
      const body = { ...form, target_et_id: etId }
      if (isNew) {
        await actionTypesApi.create(body)
      } else {
        await actionTypesApi.update(initial.id!, body)
      }
      toast.success(isNew ? 'Action 已创建' : 'Action 已更新')
      onSaved()
      onClose()
    } catch (e) {
      toast.error(String(e))
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60" onClick={onClose}>
      <div
        className="bg-slate-900 border border-slate-700 rounded-xl w-[560px] max-h-[90vh] overflow-y-auto shadow-2xl"
        onClick={e => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-5 py-4 border-b border-slate-800">
          <h3 className="text-sm font-semibold text-white">{isNew ? '新建 Action Type' : '编辑 Action Type'}</h3>
          <button onClick={onClose} className="text-slate-500 hover:text-white text-lg">✕</button>
        </div>

        <div className="p-5 space-y-4">
          {/* 基础信息 */}
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs text-slate-400 mb-1">名称（英文）*</label>
              <input
                value={form.name ?? ''}
                onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
                placeholder="CancelOrder"
                className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-xs text-white focus:outline-none focus:border-indigo-500"
              />
            </div>
            <div>
              <label className="block text-xs text-slate-400 mb-1">显示名称 *</label>
              <input
                value={form.display_name ?? ''}
                onChange={e => setForm(f => ({ ...f, display_name: e.target.value }))}
                placeholder="取消订单"
                className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-xs text-white focus:outline-none focus:border-indigo-500"
              />
            </div>
          </div>

          {/* 层级 + 触发方式 */}
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs text-slate-400 mb-1">层级</label>
              <select
                value={form.level ?? 'object'}
                onChange={e => setForm(f => ({ ...f, level: e.target.value as ActionType['level'] }))}
                className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-xs text-white focus:outline-none focus:border-indigo-500"
              >
                {Object.entries(LEVEL_LABEL).map(([v, l]) => (
                  <option key={v} value={v} className="bg-slate-900">{l}</option>
                ))}
              </select>
            </div>
            <div>
              <label className="block text-xs text-slate-400 mb-1">触发方式</label>
              <select
                value={form.trigger ?? 'manual'}
                onChange={e => setForm(f => ({ ...f, trigger: e.target.value as ActionType['trigger'] }))}
                className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-xs text-white focus:outline-none focus:border-indigo-500"
              >
                {Object.entries(TRIGGER_LABEL).map(([v, l]) => (
                  <option key={v} value={v} className="bg-slate-900">{l}</option>
                ))}
              </select>
            </div>
          </div>

          {/* 状态机 */}
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs text-slate-400 mb-1">允许的起始状态（逗号分隔）</label>
              <input
                value={(form.from_states ?? []).join(', ')}
                onChange={e => setFromStates(e.target.value)}
                placeholder="pending, confirmed"
                className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-xs text-white focus:outline-none focus:border-indigo-500"
              />
            </div>
            <div>
              <label className="block text-xs text-slate-400 mb-1">执行后状态（to_state）</label>
              <input
                value={form.to_state ?? ''}
                onChange={e => setForm(f => ({ ...f, to_state: e.target.value }))}
                placeholder="cancelled"
                className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-xs text-white focus:outline-none focus:border-indigo-500"
              />
            </div>
          </div>

          {/* 允许 Persona */}
          <div>
            <label className="block text-xs text-slate-400 mb-1">允许执行的 Persona（逗号分隔）</label>
            <input
              value={(form.allowed_personas ?? []).join(', ')}
              onChange={e => setPersonas(e.target.value)}
              placeholder="客服, 运营"
              className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-xs text-white focus:outline-none focus:border-indigo-500"
            />
          </div>

          {/* 参数列表 */}
          <div>
            <div className="flex items-center justify-between mb-2">
              <label className="text-xs text-slate-400">参数定义</label>
              <button
                onClick={addParam}
                className="text-xs text-indigo-400 hover:text-indigo-300"
              >+ 添加参数</button>
            </div>
            <div className="space-y-2">
              {(form.params ?? []).map((p, i) => (
                <div key={i} className="flex items-center gap-2 bg-slate-800/50 px-3 py-2 rounded-lg">
                  <input
                    value={p.name}
                    onChange={e => updateParam(i, { name: e.target.value })}
                    placeholder="参数名"
                    className="flex-1 bg-transparent text-xs text-white focus:outline-none placeholder-slate-600"
                  />
                  <select
                    value={p.type}
                    onChange={e => updateParam(i, { type: e.target.value })}
                    className="bg-slate-700 border border-slate-600 rounded px-2 py-1 text-xs text-slate-300 focus:outline-none"
                  >
                    {['string', 'number', 'boolean', 'date'].map(t => (
                      <option key={t} value={t} className="bg-slate-900">{t}</option>
                    ))}
                  </select>
                  <label className="flex items-center gap-1 text-xs text-slate-400">
                    <input
                      type="checkbox"
                      checked={p.required}
                      onChange={e => updateParam(i, { required: e.target.checked })}
                      className="accent-indigo-500"
                    />
                    必填
                  </label>
                  <button
                    onClick={() => removeParam(i)}
                    className="text-slate-600 hover:text-red-400 text-xs"
                  >✕</button>
                </div>
              ))}
              {(form.params ?? []).length === 0 && (
                <p className="text-xs text-slate-600 px-1">暂无参数</p>
              )}
            </div>
          </div>

          {/* 应用级 Saga 提示 */}
          {form.level === 'app' && (
            <div className="bg-amber-900/20 border border-amber-700/30 rounded-lg px-3 py-2 text-xs text-amber-400">
              应用级 Action 需要 Saga 编排引擎（Phase 3）支持，当前为声明模式，执行时会返回未实现提示。
            </div>
          )}
        </div>

        <div className="flex justify-end gap-2 px-5 py-4 border-t border-slate-800">
          <button
            onClick={onClose}
            className="px-4 py-1.5 text-xs text-slate-400 hover:text-white border border-slate-700 rounded-lg"
          >取消</button>
          <button
            onClick={save}
            disabled={saving}
            className="px-4 py-1.5 text-xs bg-indigo-600 hover:bg-indigo-500 text-white rounded-lg disabled:opacity-50"
          >{saving ? '保存中…' : (isNew ? '创建' : '保存')}</button>
        </div>
      </div>
    </div>
  )
}

// ── ActionTypesPanel（主面板）─────────────────────────────────────────────────

interface Props { etId: string }

export default function ActionTypesPanel({ etId }: Props) {
  const [open, setOpen]           = useState(false)
  const [actions, setActions]     = useState<ActionType[]>([])
  const [editing, setEditing]     = useState<Partial<ActionType> | null>(null)
  const [loading, setLoading]     = useState(false)

  async function load() {
    setLoading(true)
    try {
      setActions(await actionTypesApi.list(etId))
    } catch (_) {} finally { setLoading(false) }
  }

  useEffect(() => { if (open) load() }, [open])

  async function handleSetStatus(id: string, status: string) {
    try {
      await actionTypesApi.setStatus(id, status)
      toast.success(`状态已更新为 ${STATUS_LABEL[status]}`)
      load()
    } catch (e) { toast.error(String(e)) }
  }

  async function handleDelete(id: string, name: string) {
    if (!confirm(`确认删除 Action "${name}"？`)) return
    try {
      await actionTypesApi.delete(id)
      toast.success('已删除')
      load()
    } catch (e) { toast.error(String(e)) }
  }

  return (
    <>
      <div className="bg-slate-900 border border-slate-800 rounded-lg overflow-hidden">
        <button
          onClick={() => setOpen(o => !o)}
          className="w-full flex items-center justify-between px-4 py-3 text-sm text-slate-300 hover:bg-slate-800/40 transition-colors"
        >
          <span className="font-medium">
            Actions
            {actions.length > 0 && (
              <span className="ml-2 text-slate-500 font-normal">({actions.length})</span>
            )}
          </span>
          <div className="flex items-center gap-3">
            {open && (
              <button
                onClick={e => { e.stopPropagation(); setEditing(emptyForm()) }}
                className="text-xs text-indigo-400 hover:text-indigo-300 border border-indigo-800/50 rounded px-2 py-0.5"
              >+ 新建</button>
            )}
            <span className="text-slate-600 text-xs">{open ? '▲' : '▼'}</span>
          </div>
        </button>

        {open && (
          <div className="border-t border-slate-800">
            {loading ? (
              <p className="px-4 py-4 text-xs text-slate-600">加载中…</p>
            ) : actions.length === 0 ? (
              <div className="px-4 py-6 text-center">
                <p className="text-xs text-slate-600 mb-2">暂无 Action Type</p>
                <button
                  onClick={() => setEditing(emptyForm())}
                  className="text-xs text-indigo-400 hover:text-indigo-300"
                >+ 新建第一个</button>
              </div>
            ) : (
              <div className="divide-y divide-slate-800/50">
                {actions.map(at => (
                  <div key={at.id} className="px-4 py-3 flex items-start justify-between gap-3 hover:bg-slate-800/20 group">
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-semibold text-white">{at.display_name}</span>
                        <span className="text-[10px] text-slate-500 font-mono">{at.name}</span>
                        <span className={cn('text-[10px] px-1.5 py-0.5 rounded', STATUS_CLS[at.status])}>
                          {STATUS_LABEL[at.status]}
                        </span>
                      </div>
                      <div className="flex items-center gap-3 mt-1">
                        <span className="text-[10px] text-slate-500">{LEVEL_LABEL[at.level]}</span>
                        <span className="text-[10px] text-slate-600">·</span>
                        <span className="text-[10px] text-slate-500">{TRIGGER_LABEL[at.trigger]}</span>
                        {at.from_states.length > 0 && (
                          <>
                            <span className="text-[10px] text-slate-600">·</span>
                            <span className="text-[10px] text-slate-500">
                              [{at.from_states.join(', ')}] → {at.to_state || '—'}
                            </span>
                          </>
                        )}
                        {at.params.length > 0 && (
                          <>
                            <span className="text-[10px] text-slate-600">·</span>
                            <span className="text-[10px] text-slate-500">{at.params.length} 个参数</span>
                          </>
                        )}
                      </div>
                      {at.allowed_personas.length > 0 && (
                        <div className="flex gap-1 mt-1.5">
                          {at.allowed_personas.map(p => (
                            <span key={p} className="text-[10px] px-1.5 py-0.5 bg-slate-800 text-slate-400 rounded">{p}</span>
                          ))}
                        </div>
                      )}
                    </div>
                    <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity flex-shrink-0">
                      <button
                        onClick={() => setEditing(at)}
                        className="text-[10px] text-slate-400 hover:text-white px-2 py-1 rounded hover:bg-slate-700"
                      >编辑</button>
                      {at.status === 'draft' && (
                        <button
                          onClick={() => handleSetStatus(at.id, 'active')}
                          className="text-[10px] text-green-400 hover:text-green-300 px-2 py-1 rounded hover:bg-green-900/20"
                        >激活</button>
                      )}
                      {at.status === 'active' && (
                        <button
                          onClick={() => handleSetStatus(at.id, 'deprecated')}
                          className="text-[10px] text-amber-400 hover:text-amber-300 px-2 py-1 rounded hover:bg-amber-900/20"
                        >废弃</button>
                      )}
                      {at.status === 'deprecated' && (
                        <button
                          onClick={() => handleSetStatus(at.id, 'active')}
                          className="text-[10px] text-slate-400 hover:text-white px-2 py-1 rounded hover:bg-slate-700"
                        >恢复</button>
                      )}
                      <button
                        onClick={() => handleDelete(at.id, at.display_name)}
                        className="text-[10px] text-slate-600 hover:text-red-400 px-2 py-1 rounded"
                      >删除</button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      {editing && (
        <EditDialog
          etId={etId}
          initial={editing}
          onClose={() => setEditing(null)}
          onSaved={load}
        />
      )}
    </>
  )
}
