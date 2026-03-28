import { useState, useEffect } from 'react'
import { toast } from 'sonner'
import { stateMachineApi, objectsApi } from '@/api'
import type { StateDef, StateTransition } from '@/api/types'
import { cn } from '@/lib/utils'

// ── 颜色选项 ──────────────────────────────────────────────────────────────────
const STATE_COLORS = [
  '#6366f1', '#3b82f6', '#10b981', '#f59e0b', '#ef4444',
  '#8b5cf6', '#06b6d4', '#84cc16', '#f97316', '#ec4899',
]

// ── 从现有数据导入状态弹窗 ────────────────────────────────────────────────────
interface ImportStatesDialogProps {
  etId: string
  existingStates: StateDef[]
  onClose: () => void
  onSaved: () => void
}

function ImportStatesDialog({ etId, existingStates, onClose, onSaved }: ImportStatesDialogProps) {
  const [loading, setLoading]           = useState(true)
  const [fieldOptions, setFieldOptions] = useState<{ field: string; values: string[] }[]>([])
  const [selectedField, setSelectedField] = useState('')
  const [selected, setSelected]         = useState<Set<string>>(new Set())
  const [valueColors, setValueColors]   = useState<Record<string, string>>({})
  const [saving, setSaving]             = useState(false)

  useEffect(() => {
    objectsApi.list(etId).then(objects => {
      const fieldValues = new Map<string, Set<string>>()
      for (const obj of objects) {
        let fields: Record<string, unknown> = {}
        try { fields = typeof obj.fields === 'string' ? JSON.parse(obj.fields as string) : (obj.fields as Record<string, unknown>) } catch {}
        for (const [k, v] of Object.entries(fields)) {
          if (typeof v === 'string' && v.length > 0 && v.length < 60) {
            if (!fieldValues.has(k)) fieldValues.set(k, new Set())
            fieldValues.get(k)!.add(v)
          }
        }
      }
      const existingNames = new Set(existingStates.map(s => s.name))
      const options = [...fieldValues.entries()]
        .filter(([, vs]) => vs.size >= 2 && vs.size <= 12)
        .map(([field, vs]) => ({
          field,
          values: [...vs].filter(v => !existingNames.has(v)),
        }))
        .filter(o => o.values.length > 0)

      setFieldOptions(options)
      if (options.length > 0) {
        setSelectedField(options[0].field)
        const colors: Record<string, string> = {}
        options[0].values.forEach((v, i) => { colors[v] = STATE_COLORS[i % STATE_COLORS.length] })
        setValueColors(colors)
      }
    }).catch(() => {}).finally(() => setLoading(false))
  }, [etId])

  function handleFieldChange(field: string) {
    setSelectedField(field)
    setSelected(new Set())
    const option = fieldOptions.find(o => o.field === field)
    if (!option) return
    const colors: Record<string, string> = {}
    option.values.forEach((v, i) => { colors[v] = STATE_COLORS[i % STATE_COLORS.length] })
    setValueColors(colors)
  }

  const currentValues = fieldOptions.find(o => o.field === selectedField)?.values ?? []

  async function handleImport() {
    if (!selected.size) return
    setSaving(true)
    try {
      for (const v of selected) {
        await stateMachineApi.createState(etId, {
          name: v,
          display_name: v,
          color: valueColors[v] ?? '#6366f1',
          is_initial: false,
          is_terminal: false,
        })
      }
      toast.success(`已导入 ${selected.size} 个状态`)
      onSaved(); onClose()
    } catch (e) { toast.error(String(e)) } finally { setSaving(false) }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60" onClick={onClose}>
      <div className="bg-slate-900 border border-slate-700 rounded-xl w-[460px] shadow-2xl" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between px-5 py-4 border-b border-slate-800">
          <div>
            <h3 className="text-sm font-semibold text-white">从现有数据导入状态</h3>
            <p className="text-[10px] text-slate-500 mt-0.5">自动检测对象字段中的枚举值</p>
          </div>
          <button onClick={onClose} className="text-slate-500 hover:text-white">✕</button>
        </div>

        {loading ? (
          <p className="px-5 py-10 text-xs text-slate-500 text-center">分析对象数据中…</p>
        ) : fieldOptions.length === 0 ? (
          <p className="px-5 py-10 text-xs text-slate-500 text-center">
            未发现可导入的枚举字段<br />
            <span className="text-slate-600">（需要有 2–12 个不同值的字符串字段）</span>
          </p>
        ) : (
          <div className="p-5 space-y-4">
            {/* 字段选择 */}
            <div>
              <label className="block text-xs text-slate-400 mb-1.5">选择状态字段</label>
              <select
                value={selectedField}
                onChange={e => handleFieldChange(e.target.value)}
                className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-2 text-xs text-white focus:outline-none focus:border-indigo-500"
              >
                {fieldOptions.map(o => (
                  <option key={o.field} value={o.field} className="bg-slate-900">
                    {o.field}（{o.values.length} 个枚举值）
                  </option>
                ))}
              </select>
            </div>

            {/* 值列表 */}
            <div>
              <div className="flex items-center justify-between mb-2">
                <label className="text-xs text-slate-400">勾选要创建为状态的值</label>
                <button onClick={() => setSelected(new Set(currentValues))}
                  className="text-[10px] text-indigo-400 hover:text-indigo-300">全选</button>
              </div>
              <div className="space-y-2 max-h-52 overflow-y-auto">
                {currentValues.map(v => (
                  <label key={v} className="flex items-center gap-3 cursor-pointer group px-2 py-1.5 rounded-lg hover:bg-slate-800/50">
                    <input type="checkbox" checked={selected.has(v)}
                      onChange={() => setSelected(s => { const n = new Set(s); n.has(v) ? n.delete(v) : n.add(v); return n })}
                      className="accent-indigo-500 flex-shrink-0" />
                    <span className="w-5 h-5 rounded-full flex-shrink-0 border border-white/20"
                      style={{ backgroundColor: valueColors[v] ?? '#6366f1' }} />
                    <span className="text-xs text-slate-200 flex-1 font-mono">{v}</span>
                    {/* 颜色选择（hover 展示）*/}
                    <div className="flex gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                      {STATE_COLORS.map(c => (
                        <button key={c} onClick={e => { e.preventDefault(); setValueColors(vc => ({ ...vc, [v]: c })) }}
                          className={cn('w-3.5 h-3.5 rounded-full border', valueColors[v] === c ? 'border-white scale-110' : 'border-transparent')}
                          style={{ backgroundColor: c }} />
                      ))}
                    </div>
                  </label>
                ))}
              </div>
            </div>
          </div>
        )}

        <div className="flex justify-end gap-2 px-5 py-4 border-t border-slate-800">
          <button onClick={onClose} className="px-4 py-1.5 text-xs text-slate-400 border border-slate-700 rounded-lg hover:text-white">取消</button>
          <button onClick={handleImport} disabled={!selected.size || saving || loading}
            className="px-4 py-1.5 text-xs bg-indigo-600 hover:bg-indigo-500 text-white rounded-lg disabled:opacity-50">
            {saving ? '导入中…' : `导入 ${selected.size} 个状态`}
          </button>
        </div>
      </div>
    </div>
  )
}

// ── 新建/编辑状态弹窗 ─────────────────────────────────────────────────────────
interface StateDialogProps {
  etId: string
  initial?: StateDef   // 有值则为编辑模式
  onClose: () => void
  onSaved: () => void
}

function StateDialog({ etId, initial, onClose, onSaved }: StateDialogProps) {
  const isEdit = !!initial
  const [form, setForm] = useState({
    name:         initial?.name         ?? '',
    display_name: initial?.display_name ?? '',
    color:        initial?.color        ?? '#6366f1',
    is_initial:   initial?.is_initial   ?? false,
    is_terminal:  initial?.is_terminal  ?? false,
    description:  initial?.description  ?? '',
  })
  const [saving, setSaving] = useState(false)

  async function save() {
    if (!form.display_name.trim() || (!isEdit && !form.name.trim())) { toast.error('名称不能为空'); return }
    setSaving(true)
    try {
      if (isEdit) {
        await stateMachineApi.updateState(initial!.id, form)
        toast.success('状态已更新')
      } else {
        await stateMachineApi.createState(etId, form)
        toast.success('状态已添加')
      }
      onSaved(); onClose()
    } catch (e) { toast.error(String(e)) } finally { setSaving(false) }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60" onClick={onClose}>
      <div className="bg-slate-900 border border-slate-700 rounded-xl w-[400px] shadow-2xl" onClick={e => e.stopPropagation()}>
        <div className="flex items-center justify-between px-5 py-4 border-b border-slate-800">
          <h3 className="text-sm font-semibold text-white">{isEdit ? '编辑状态' : '新建状态'}</h3>
          <button onClick={onClose} className="text-slate-500 hover:text-white">✕</button>
        </div>
        <div className="p-5 space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="block text-xs text-slate-400 mb-1">名称（英文）{!isEdit && '*'}</label>
              <input value={form.name} onChange={e => setForm(f => ({ ...f, name: e.target.value }))}
                placeholder="pending"
                disabled={isEdit}
                className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-xs text-white focus:outline-none focus:border-indigo-500 disabled:opacity-50 disabled:cursor-not-allowed" />
            </div>
            <div>
              <label className="block text-xs text-slate-400 mb-1">显示名称 *</label>
              <input value={form.display_name} onChange={e => setForm(f => ({ ...f, display_name: e.target.value }))}
                placeholder="待处理"
                className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-xs text-white focus:outline-none focus:border-indigo-500" />
            </div>
          </div>
          <div>
            <label className="block text-xs text-slate-400 mb-1">颜色</label>
            <div className="flex gap-2 flex-wrap">
              {STATE_COLORS.map(c => (
                <button key={c} onClick={() => setForm(f => ({ ...f, color: c }))}
                  className={cn('w-6 h-6 rounded-full border-2 transition-all', form.color === c ? 'border-white scale-110' : 'border-transparent')}
                  style={{ backgroundColor: c }} />
              ))}
            </div>
          </div>
          <div className="flex gap-4">
            <label className="flex items-center gap-2 text-xs text-slate-300 cursor-pointer">
              <input type="checkbox" checked={form.is_initial} onChange={e => setForm(f => ({ ...f, is_initial: e.target.checked }))} className="accent-indigo-500" />
              初始状态
            </label>
            <label className="flex items-center gap-2 text-xs text-slate-300 cursor-pointer">
              <input type="checkbox" checked={form.is_terminal} onChange={e => setForm(f => ({ ...f, is_terminal: e.target.checked }))} className="accent-indigo-500" />
              终止状态
            </label>
          </div>
          <div>
            <label className="block text-xs text-slate-400 mb-1">说明（可选）</label>
            <input value={form.description} onChange={e => setForm(f => ({ ...f, description: e.target.value }))}
              placeholder="订单等待确认中"
              className="w-full bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-xs text-white focus:outline-none focus:border-indigo-500" />
          </div>
        </div>
        <div className="flex justify-end gap-2 px-5 py-4 border-t border-slate-800">
          <button onClick={onClose} className="px-4 py-1.5 text-xs text-slate-400 border border-slate-700 rounded-lg hover:text-white">取消</button>
          <button onClick={save} disabled={saving} className="px-4 py-1.5 text-xs bg-indigo-600 hover:bg-indigo-500 text-white rounded-lg disabled:opacity-50">
            {saving ? '保存…' : (isEdit ? '保存' : '创建')}
          </button>
        </div>
      </div>
    </div>
  )
}

// ── 状态图（拓扑层级布局 + 贝塞尔箭头）──────────────────────────────────────
interface StateDiagramProps { states: StateDef[]; transitions: StateTransition[] }

function StateDiagram({ states, transitions }: StateDiagramProps) {
  if (states.length === 0) return null

  const NW = 112, NH = 36, GAP_X = 80, GAP_Y = 18, PAD = 16

  // ── 1. 拓扑层级：最长路径算法（迭代 + 防环）─────────────────────────────
  const inEdges = new Map<string, string[]>(states.map(s => [s.id, []]))
  for (const t of transitions) {
    inEdges.get(t.to_state_id)?.push(t.from_state_id)
  }
  const layer = new Map<string, number>(states.map(s => [s.id, 0]))
  let changed = true, iter = 0
  while (changed && iter++ < states.length * 2) {
    changed = false
    for (const s of states) {
      const preds = inEdges.get(s.id) ?? []
      if (!preds.length) continue
      const maxPred = Math.max(...preds.map(p => layer.get(p) ?? 0))
      const next = maxPred + 1
      if (next > (layer.get(s.id) ?? 0)) { layer.set(s.id, next); changed = true }
    }
  }

  // ── 2. 按层分组并排列位置 ────────────────────────────────────────────────
  const layerGroups = new Map<number, string[]>()
  for (const [id, l] of layer) {
    if (!layerGroups.has(l)) layerGroups.set(l, [])
    layerGroups.get(l)!.push(id)
  }
  const numLayers = Math.max(...layerGroups.keys()) + 1
  const maxPerLayer = Math.max(...[...layerGroups.values()].map(v => v.length))

  const svgW = PAD * 2 + numLayers * NW + (numLayers - 1) * GAP_X
  const svgH = PAD * 2 + maxPerLayer * NH + (maxPerLayer - 1) * GAP_Y

  const pos = new Map<string, { x: number; y: number; cx: number; cy: number }>()
  for (const [l, ids] of layerGroups) {
    const x = PAD + l * (NW + GAP_X)
    const totalH = ids.length * NH + (ids.length - 1) * GAP_Y
    const startY = (svgH - totalH) / 2
    ids.forEach((id, i) => {
      const y = startY + i * (NH + GAP_Y)
      pos.set(id, { x, y, cx: x + NW / 2, cy: y + NH / 2 })
    })
  }

  // ── 3. 为同一 from 的多条边分配垂直偏移（防箭头重叠）────────────────────
  const fromCount = new Map<string, number>()
  for (const t of transitions) {
    fromCount.set(t.from_state_id, (fromCount.get(t.from_state_id) ?? 0) + 1)
  }
  const fromCur = new Map<string, number>()
  const edgeOffsets = transitions.map(t => {
    const total = fromCount.get(t.from_state_id) ?? 1
    const cur   = fromCur.get(t.from_state_id) ?? 0
    fromCur.set(t.from_state_id, cur + 1)
    // offset: spread edges evenly [-step, 0, +step, ...]
    const step = 8
    const offset = total === 1 ? 0 : (cur - (total - 1) / 2) * step
    return offset
  })

  return (
    <div className="overflow-x-auto">
      <svg width={svgW} height={svgH} className="block">
        <defs>
          <marker id="sm-arrow" markerWidth="9" markerHeight="7" refX="8" refY="3.5" orient="auto">
            <polygon points="0 0, 9 3.5, 0 7" fill="#64748b" />
          </marker>
        </defs>

        {/* ── Edges ── */}
        {transitions.map((t, i) => {
          const from = pos.get(t.from_state_id)
          const to   = pos.get(t.to_state_id)
          if (!from || !to) return null
          const fromS = states.find(s => s.id === t.from_state_id)
          const offset = edgeOffsets[i]

          // Exit right-center of from, enter left-center of to
          const x1 = from.x + NW, y1 = from.cy + offset
          const x2 = to.x - 1,   y2 = to.cy + offset
          const dx = (x2 - x1) * 0.45
          const d = `M ${x1} ${y1} C ${x1 + dx} ${y1}, ${x2 - dx} ${y2}, ${x2} ${y2}`

          return (
            <g key={t.id}>
              <path d={d} fill="none"
                stroke={fromS?.color ?? '#475569'} strokeOpacity={0.5}
                strokeWidth={1.5} markerEnd="url(#sm-arrow)" />
            </g>
          )
        })}

        {/* ── Nodes ── */}
        {states.map(s => {
          const p = pos.get(s.id)
          if (!p) return null
          return (
            <g key={s.id}>
              {/* shadow */}
              <rect x={p.x + 2} y={p.y + 2} width={NW} height={NH} rx={8}
                fill={s.color} fillOpacity={0.08} />
              {/* body */}
              <rect x={p.x} y={p.y} width={NW} height={NH} rx={8}
                fill={s.color + '20'} stroke={s.color}
                strokeWidth={s.is_initial || s.is_terminal ? 2 : 1.5} />
              {/* initial dot */}
              {s.is_initial && (
                <circle cx={p.x + 11} cy={p.cy} r={3.5} fill={s.color} />
              )}
              {/* terminal double-ring */}
              {s.is_terminal && (
                <>
                  <circle cx={p.x + NW - 11} cy={p.cy} r={5} fill="none" stroke={s.color} strokeWidth={1.5} />
                  <circle cx={p.x + NW - 11} cy={p.cy} r={2.5} fill={s.color} />
                </>
              )}
              {/* label */}
              <text x={p.cx} y={p.cy + 4} textAnchor="middle"
                fontSize={11} fill={s.color} fontWeight={600} letterSpacing={0.3}>
                {s.display_name}
              </text>
            </g>
          )
        })}
      </svg>
    </div>
  )
}

// ── StateMachinePanel（主面板）───────────────────────────────────────────────

interface Props { etId: string }

export default function StateMachinePanel({ etId }: Props) {
  const [open, setOpen]                   = useState(false)
  const [states, setStates]               = useState<StateDef[]>([])
  const [transitions, setTransitions]     = useState<StateTransition[]>([])
  const [stateDialog, setStateDialog]     = useState<StateDef | true | null>(null) // true=新建, StateDef=编辑
  const [showImport, setShowImport]       = useState(false)
  const [addingTrans, setAddingTrans]     = useState(false)
  const [fromId, setFromId]               = useState('')
  const [toId, setToId]                   = useState('')
  const [loading, setLoading]             = useState(false)

  async function load() {
    setLoading(true)
    try {
      const [s, t] = await Promise.all([
        stateMachineApi.listStates(etId),
        stateMachineApi.listTransitions(etId),
      ])
      setStates(s); setTransitions(t)
    } catch (_) {} finally { setLoading(false) }
  }

  useEffect(() => { if (open) load() }, [open, etId])

  async function handleAddTransition() {
    if (!fromId || !toId || fromId === toId) { toast.error('请选择不同的起止状态'); return }
    try {
      await stateMachineApi.createTransition(etId, fromId, toId)
      toast.success('转换规则已添加')
      setFromId(''); setToId(''); setAddingTrans(false)
      load()
    } catch (e) { toast.error(String(e)) }
  }

  async function handleDeleteState(id: string, name: string) {
    if (!confirm(`删除状态 "${name}"？相关转换规则也会一并删除。`)) return
    try { await stateMachineApi.deleteState(id); toast.success('已删除'); load() }
    catch (e) { toast.error(String(e)) }
  }

  async function handleDeleteTransition(id: string) {
    try { await stateMachineApi.deleteTransition(id); load() }
    catch (e) { toast.error(String(e)) }
  }

  return (
    <>
      <div className="bg-slate-900 border border-slate-800 rounded-lg overflow-hidden">
        <button
          onClick={() => setOpen(o => !o)}
          className="w-full flex items-center justify-between px-4 py-3 text-sm text-slate-300 hover:bg-slate-800/40 transition-colors"
        >
          <span className="font-medium">
            状态机
            {states.length > 0 && <span className="ml-2 text-slate-500 font-normal">({states.length} 个状态 · {transitions.length} 条转换)</span>}
          </span>
          <div className="flex items-center gap-3">
            {open && (
              <>
                <button onClick={e => { e.stopPropagation(); setShowImport(true) }}
                  className="text-xs text-slate-400 hover:text-slate-200 border border-slate-700 rounded px-2 py-0.5">
                  从数据导入
                </button>
                <button onClick={e => { e.stopPropagation(); setStateDialog(true) }}
                  className="text-xs text-indigo-400 hover:text-indigo-300 border border-indigo-800/50 rounded px-2 py-0.5">
                  + 新建状态
                </button>
              </>
            )}
            <span className="text-slate-600 text-xs">{open ? '▲' : '▼'}</span>
          </div>
        </button>

        {open && (
          <div className="border-t border-slate-800 p-4 space-y-4">
            {loading ? (
              <p className="text-xs text-slate-600">加载中…</p>
            ) : (
              <>
                {/* 状态图 */}
                {states.length > 0 && (
                  <div className="bg-slate-800/30 rounded-lg p-3">
                    <StateDiagram states={states} transitions={transitions} />
                  </div>
                )}

                {/* 状态列表 */}
                <div>
                  <p className="text-xs text-slate-500 mb-2 font-medium uppercase tracking-wider">状态定义</p>
                  {states.length === 0 ? (
                    <p className="text-xs text-slate-600">暂无状态 — 点击"新建状态"开始</p>
                  ) : (
                    <div className="flex flex-wrap gap-2">
                      {states.map(s => (
                        <div key={s.id} className="flex items-center gap-1.5 px-2.5 py-1 rounded-full border group"
                          style={{ borderColor: s.color + '60', backgroundColor: s.color + '15' }}>
                          <span className="w-2 h-2 rounded-full flex-shrink-0" style={{ backgroundColor: s.color }} />
                          <span className="text-xs font-medium" style={{ color: s.color }}>{s.display_name}</span>
                          <span className="text-[10px] text-slate-600 font-mono">({s.name})</span>
                          {s.is_initial && <span className="text-[9px] text-indigo-400">初始</span>}
                          {s.is_terminal && <span className="text-[9px] text-slate-500">终止</span>}
                          <button onClick={() => setStateDialog(s)}
                            className="opacity-0 group-hover:opacity-100 text-slate-600 hover:text-slate-300 text-[10px] ml-0.5 transition-opacity">✎</button>
                          <button onClick={() => handleDeleteState(s.id, s.display_name)}
                            className="opacity-0 group-hover:opacity-100 text-slate-600 hover:text-red-400 text-[10px] transition-opacity">✕</button>
                        </div>
                      ))}
                    </div>
                  )}
                </div>

                {/* 转换规则 */}
                {states.length >= 2 && (
                  <div>
                    <div className="flex items-center justify-between mb-2">
                      <p className="text-xs text-slate-500 font-medium uppercase tracking-wider">转换规则</p>
                      {!addingTrans && (
                        <button onClick={() => setAddingTrans(true)}
                          className="text-xs text-indigo-400 hover:text-indigo-300">+ 添加转换</button>
                      )}
                    </div>

                    {addingTrans && (
                      <div className="flex items-center gap-2 mb-3 bg-slate-800/40 px-3 py-2 rounded-lg">
                        <select value={fromId} onChange={e => setFromId(e.target.value)}
                          className="flex-1 bg-slate-800 border border-slate-700 rounded px-2 py-1 text-xs text-white focus:outline-none">
                          <option value="">起始状态…</option>
                          {states.map(s => <option key={s.id} value={s.id} className="bg-slate-900">{s.display_name}</option>)}
                        </select>
                        <span className="text-slate-500 text-xs flex-shrink-0">→</span>
                        <select value={toId} onChange={e => setToId(e.target.value)}
                          className="flex-1 bg-slate-800 border border-slate-700 rounded px-2 py-1 text-xs text-white focus:outline-none">
                          <option value="">目标状态…</option>
                          {states.filter(s => s.id !== fromId).map(s => <option key={s.id} value={s.id} className="bg-slate-900">{s.display_name}</option>)}
                        </select>
                        <button onClick={handleAddTransition}
                          className="text-xs bg-indigo-600 hover:bg-indigo-500 text-white px-2 py-1 rounded flex-shrink-0">确认</button>
                        <button onClick={() => { setAddingTrans(false); setFromId(''); setToId('') }}
                          className="text-xs text-slate-500 hover:text-white flex-shrink-0">取消</button>
                      </div>
                    )}

                    <div className="space-y-1">
                      {transitions.length === 0 ? (
                        <p className="text-xs text-slate-600">暂无转换规则</p>
                      ) : transitions.map(t => (
                        <div key={t.id} className="flex items-center gap-2 group py-1">
                          <span className="text-xs px-2 py-0.5 rounded-full" style={{ color: t.from_color, backgroundColor: t.from_color + '20' }}>{t.from_display}</span>
                          <span className="text-slate-600 text-xs">→</span>
                          <span className="text-xs px-2 py-0.5 rounded-full" style={{ color: t.to_color, backgroundColor: t.to_color + '20' }}>{t.to_display}</span>
                          <button onClick={() => handleDeleteTransition(t.id)}
                            className="opacity-0 group-hover:opacity-100 text-slate-600 hover:text-red-400 text-[10px] ml-1 transition-opacity">✕</button>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </>
            )}
          </div>
        )}
      </div>

      {stateDialog && (
        <StateDialog
          etId={etId}
          initial={stateDialog === true ? undefined : stateDialog}
          onClose={() => setStateDialog(null)}
          onSaved={load}
        />
      )}

      {showImport && (
        <ImportStatesDialog
          etId={etId}
          existingStates={states}
          onClose={() => setShowImport(false)}
          onSaved={load}
        />
      )}
    </>
  )
}
