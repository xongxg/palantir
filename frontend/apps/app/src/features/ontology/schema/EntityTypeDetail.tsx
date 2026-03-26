import { useState, useEffect } from 'react'
import { toast } from 'sonner'
import { useSchemaStore } from '@/store/schemaStore'
import { entityTypesApi, interfacesApi } from '@/api'
import type { BreakingChangeInfo, EtLineage, Interface } from '@/api/types'
import AddFieldDialog from './AddFieldDialog'
import { cn } from '@/lib/utils'

const CLASSIFICATION_COLORS: Record<string, string> = {
  PII:       'text-red-400 bg-red-950/40',
  Sensitive: 'text-amber-400 bg-amber-950/40',
  Public:    'text-green-400 bg-green-950/40',
  Internal:  'text-slate-400 bg-slate-800/40',
}

const DDD_ROLES = ['entity', 'aggregate_root', 'value_object', 'domain_event', 'service']
const DDD_ROLE_LABEL: Record<string, string> = {
  entity: 'Entity', aggregate_root: 'Aggregate Root',
  value_object: 'Value Object', domain_event: 'Domain Event', service: 'Service',
}
const DDD_ROLE_COLOR: Record<string, string> = {
  aggregate_root: 'text-red-400', value_object: 'text-green-400',
  domain_event: 'text-amber-400', service: 'text-purple-400', entity: 'text-blue-400',
}

const STATUS_CONFIG: Record<string, { label: string; cls: string; nextActions: { label: string; next: string }[] }> = {
  draft:      { label: '草稿', cls: 'text-slate-400 bg-slate-800',     nextActions: [{ label: '发布', next: 'active' }] },
  active:     { label: '活跃', cls: 'text-green-400 bg-green-900/40',  nextActions: [{ label: '废弃', next: 'deprecated' }] },
  deprecated: { label: '已废弃', cls: 'text-amber-400 bg-amber-900/40', nextActions: [{ label: '恢复', next: 'active' }] },
}

// ── Breaking Change Confirm Modal ─────────────────────────────────────────────

interface BreakingModalProps {
  info: BreakingChangeInfo
  onConfirm: (strategy: string) => void
  onCancel: () => void
}

function BreakingChangeModal({ info, onConfirm, onCancel }: BreakingModalProps) {
  const [strategy, setStrategy] = useState(info.strategies[0] ?? 'drop')

  const STRATEGY_LABELS: Record<string, string> = {
    drop: '丢弃 — 清空所有已有对象中该字段的值',
    cast: '转换 — 尝试将字段值转为新类型（失败则为空）',
    rename: '重命名 — 将旧字段值迁移到新字段名',
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="bg-slate-900 border border-amber-700/50 rounded-xl p-6 w-[480px] shadow-2xl space-y-4">
        <div className="flex items-start gap-3">
          <span className="text-amber-400 text-xl mt-0.5">⚠</span>
          <div>
            <h3 className="font-semibold text-white">Breaking Change 警告</h3>
            <p className="text-sm text-slate-400 mt-1">
              {info.change_type === 'type_change'
                ? `将字段类型从 ${info.old_value} 改为 ${info.new_value}`
                : `删除字段「${info.old_value}」`}
            </p>
            <p className="text-sm text-amber-300 mt-1">
              当前有 <strong>{info.affected_count.toLocaleString()}</strong> 条 Ontology 对象包含此字段
            </p>
          </div>
        </div>

        <div className="space-y-2">
          <p className="text-xs text-slate-500 font-medium uppercase tracking-wide">选择迁移策略</p>
          {info.strategies.map(s => (
            <label key={s} className={cn(
              'flex items-start gap-3 p-3 rounded-lg cursor-pointer border transition-colors',
              strategy === s
                ? 'border-indigo-600 bg-indigo-900/30'
                : 'border-slate-700 hover:border-slate-600',
            )}>
              <input
                type="radio" name="strategy" value={s}
                checked={strategy === s}
                onChange={() => setStrategy(s)}
                className="mt-0.5"
              />
              <span className="text-sm text-slate-300">{STRATEGY_LABELS[s] ?? s}</span>
            </label>
          ))}
        </div>

        <div className="flex justify-end gap-2 pt-1">
          <button onClick={onCancel} className="px-4 py-1.5 text-sm text-slate-400 hover:text-white transition-colors">
            取消
          </button>
          <button
            onClick={() => onConfirm(strategy)}
            className="px-4 py-1.5 text-sm bg-amber-600 hover:bg-amber-500 text-white rounded-lg transition-colors"
          >
            确认执行迁移
          </button>
        </div>
      </div>
    </div>
  )
}

// ── Lineage Panel ──────────────────────────────────────────────────────────────

function LineagePanel({ etId }: { etId: string }) {
  const [lineage, setLineage] = useState<EtLineage | null>(null)
  const [open, setOpen] = useState(false)

  useEffect(() => {
    if (open && !lineage) {
      entityTypesApi.getLineage(etId).then(setLineage).catch(() => {})
    }
  }, [open, etId])

  return (
    <div className="bg-slate-900 border border-slate-800 rounded-lg overflow-hidden">
      <button
        onClick={() => setOpen(o => !o)}
        className="w-full flex items-center justify-between px-4 py-3 text-sm text-slate-300 hover:bg-slate-800/40 transition-colors"
      >
        <span className="font-medium">
          数据来源
          {lineage && (
            <span className="ml-2 text-slate-500 font-normal">({lineage.sources.length} 个 Dataset)</span>
          )}
        </span>
        <span className="text-slate-600 text-xs">{open ? '▲' : '▼'}</span>
      </button>

      {open && (
        <div className="border-t border-slate-800">
          {!lineage ? (
            <p className="px-4 py-4 text-xs text-slate-600">加载中…</p>
          ) : lineage.sources.length === 0 ? (
            <p className="px-4 py-4 text-xs text-slate-600">暂无数据源映射</p>
          ) : (
            <>
              <div className="divide-y divide-slate-800/50">
                {lineage.sources.map(s => (
                  <div key={s.dataset_id} className="px-4 py-3 flex items-start justify-between gap-4">
                    <div className="min-w-0">
                      <p className="text-sm text-slate-200 truncate font-medium">
                        <span className="mr-1.5 text-slate-600">📄</span>{s.dataset_name}
                      </p>
                      <p className="text-xs text-slate-500 mt-0.5">
                        {s.fold_name} · {s.source_name}
                        {s.last_synced_at && (
                          <span className="ml-2 text-slate-700">· 最后同步 {s.last_synced_at.slice(0, 10)}</span>
                        )}
                      </p>
                    </div>
                    <div className="text-right flex-shrink-0">
                      <p className="text-sm text-slate-300">{s.record_count.toLocaleString()} 条</p>
                      <p className="text-[10px] text-slate-600 mt-0.5">{s.sync_mode}</p>
                    </div>
                  </div>
                ))}
              </div>
              <div className="px-4 py-2.5 border-t border-slate-800 flex items-center justify-between">
                <span className="text-xs text-slate-600">合计</span>
                <span className="text-sm font-medium text-slate-300">{lineage.total_records.toLocaleString()} 条</span>
              </div>
            </>
          )}
        </div>
      )}
    </div>
  )
}

// ── Interfaces Panel ───────────────────────────────────────────────────────────

function InterfacesPanel({ etId }: { etId: string }) {
  const [etInterfaces, setEtInterfaces] = useState<Interface[]>([])
  const [allInterfaces, setAllInterfaces] = useState<Interface[]>([])
  const [open, setOpen] = useState(false)
  const [showAdd, setShowAdd] = useState(false)

  useEffect(() => {
    if (open) {
      entityTypesApi.listInterfaces(etId).then(setEtInterfaces).catch(() => {})
      interfacesApi.list().then(setAllInterfaces).catch(() => {})
    }
  }, [open, etId])

  async function handleAdd(interfaceId: string) {
    try {
      await entityTypesApi.addInterface(etId, interfaceId)
      const updated = await entityTypesApi.listInterfaces(etId)
      setEtInterfaces(updated)
      toast.success('Interface 已添加')
    } catch (e) {
      toast.error(String(e))
    }
  }

  async function handleRemove(interfaceId: string) {
    try {
      await entityTypesApi.removeInterface(etId, interfaceId)
      setEtInterfaces(prev => prev.filter(i => i.id !== interfaceId))
      toast.success('Interface 已移除')
    } catch (e) {
      toast.error(String(e))
    }
  }

  const assignedIds = new Set(etInterfaces.map(i => i.id))
  const available = allInterfaces.filter(i => !assignedIds.has(i.id))

  return (
    <div className="bg-slate-900 border border-slate-800 rounded-lg overflow-hidden">
      <button
        onClick={() => setOpen(o => !o)}
        className="w-full flex items-center justify-between px-4 py-3 text-sm text-slate-300 hover:bg-slate-800/40 transition-colors"
      >
        <span className="font-medium">
          System Interface
          {etInterfaces.length > 0 && (
            <span className="ml-2 text-slate-500 font-normal">({etInterfaces.length})</span>
          )}
        </span>
        <span className="text-slate-600 text-xs">{open ? '▲' : '▼'}</span>
      </button>

      {open && (
        <div className="border-t border-slate-800 p-4 space-y-3">
          {etInterfaces.length === 0 ? (
            <p className="text-xs text-slate-600">未实现任何 Interface</p>
          ) : (
            <div className="space-y-2">
              {etInterfaces.map(iface => (
                <div key={iface.id} className="flex items-start justify-between gap-2 p-3 bg-slate-800/40 rounded-lg">
                  <div className="min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm text-slate-200 font-medium">{iface.name}</span>
                      {iface.is_builtin && (
                        <span className="text-[9px] px-1 py-0.5 rounded bg-slate-700 text-slate-500">内置</span>
                      )}
                    </div>
                    {iface.description && (
                      <p className="text-xs text-slate-600 mt-0.5">{iface.description}</p>
                    )}
                    <div className="flex flex-wrap gap-1 mt-1.5">
                      {iface.fields.map(f => (
                        <span key={f.field_name} className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-slate-800 text-slate-500">
                          {f.field_name}: {f.field_type}
                        </span>
                      ))}
                    </div>
                  </div>
                  <button
                    onClick={() => handleRemove(iface.id)}
                    className="text-slate-700 hover:text-red-400 transition-colors text-xs flex-shrink-0 mt-0.5"
                  >✕</button>
                </div>
              ))}
            </div>
          )}

          {!showAdd ? (
            <button
              onClick={() => setShowAdd(true)}
              className="text-xs text-indigo-400 hover:text-indigo-300 transition-colors"
            >
              + 添加 Interface
            </button>
          ) : (
            <div className="space-y-1.5">
              {available.length === 0 ? (
                <p className="text-xs text-slate-600">已全部添加</p>
              ) : available.map(iface => (
                <button
                  key={iface.id}
                  onClick={() => { handleAdd(iface.id); setShowAdd(false) }}
                  className="w-full text-left flex items-center gap-2 px-3 py-2 rounded-lg bg-slate-800/60 hover:bg-slate-800 transition-colors"
                >
                  <div className="min-w-0 flex-1">
                    <p className="text-sm text-slate-200">{iface.name}</p>
                    {iface.description && (
                      <p className="text-xs text-slate-600 truncate">{iface.description}</p>
                    )}
                  </div>
                  {iface.is_builtin && (
                    <span className="text-[9px] px-1 py-0.5 rounded bg-slate-700 text-slate-500 flex-shrink-0">内置</span>
                  )}
                </button>
              ))}
              <button onClick={() => setShowAdd(false)} className="text-xs text-slate-600 hover:text-slate-400 transition-colors">
                取消
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

// ── Main Component ─────────────────────────────────────────────────────────────

export default function EntityTypeDetail() {
  const { entityTypes, selectedId, removeEntityType, upsertEntityType } = useSchemaStore()
  const [showAddField, setShowAddField] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [statusLoading, setStatusLoading] = useState(false)
  const [roleLoading, setRoleLoading] = useState(false)
  // Breaking change state
  const [breakingInfo, setBreakingInfo] = useState<(BreakingChangeInfo & { fieldId: string; fieldName: string; newType?: string }) | null>(null)

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

  const statusCfg = STATUS_CONFIG[et.status ?? 'active'] ?? STATUS_CONFIG.active

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
    // P0: check breaking first
    try {
      const info = await entityTypesApi.checkDeleteField(fieldId)
      if (info.breaking) {
        setBreakingInfo({ ...info, fieldId, fieldName })
        return
      }
    } catch (_) {}
    // Not breaking or check failed — direct delete
    try {
      await entityTypesApi.deleteField(fieldId)
      upsertEntityType({ ...et!, fields: et!.fields.filter(f => f.id !== fieldId) })
      toast.success('字段已删除')
    } catch (e) {
      toast.error(String(e))
    }
  }

  async function handleBreakingConfirm(strategy: string) {
    if (!breakingInfo) return
    try {
      await entityTypesApi.deleteFieldSafe(breakingInfo.fieldId)
      upsertEntityType({ ...et!, fields: et!.fields.filter(f => f.id !== breakingInfo.fieldId) })
      toast.success(`字段已删除（策略: ${strategy}），${breakingInfo.affected_count} 条对象已迁移`)
    } catch (e) {
      toast.error(String(e))
    } finally {
      setBreakingInfo(null)
    }
  }

  async function handleSetStatus(nextStatus: string) {
    setStatusLoading(true)
    try {
      const res = await entityTypesApi.setStatus(et!.id, nextStatus)
      upsertEntityType({ ...et!, status: nextStatus })
      if (res.affected_datasets > 0) {
        toast.success(`状态已更新为「${STATUS_CONFIG[nextStatus]?.label}」，${res.affected_datasets} 个 Dataset 受影响`)
      } else {
        toast.success(`状态已更新为「${STATUS_CONFIG[nextStatus]?.label}」`)
      }
    } catch (e) {
      toast.error(String(e))
    } finally {
      setStatusLoading(false)
    }
  }

  async function handleSetDddRole(role: string) {
    setRoleLoading(true)
    try {
      await entityTypesApi.setDddRole(et!.id, role)
      upsertEntityType({ ...et!, ddd_role: role })
      toast.success(`DDD 角色已更新为「${DDD_ROLE_LABEL[role] ?? role}」`)
    } catch (e) {
      toast.error(String(e))
    } finally {
      setRoleLoading(false)
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
      {/* Breaking change modal */}
      {breakingInfo && (
        <BreakingChangeModal
          info={breakingInfo}
          onConfirm={handleBreakingConfirm}
          onCancel={() => setBreakingInfo(null)}
        />
      )}

      {/* Header */}
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-center gap-3">
          <span className="text-4xl" style={{ color: et.color }}>{et.icon || '●'}</span>
          <div>
            <div className="flex items-center gap-2">
              <h2 className="text-xl font-bold text-white">{et.display_name || et.name}</h2>
              <span className={cn('text-[10px] px-1.5 py-0.5 rounded font-medium', statusCfg.cls)}>
                {statusCfg.label}
              </span>
            </div>
            <div className="flex items-center gap-2 mt-0.5">
              <p className="text-xs text-slate-500 font-mono">
                {et.name}
                {et.namespace && (
                  <span className="ml-2 text-slate-600">· {et.namespace}</span>
                )}
              </p>
              <select
                value={et.ddd_role || 'entity'}
                onChange={e => handleSetDddRole(e.target.value)}
                disabled={roleLoading}
                className={cn(
                  'text-[10px] font-semibold px-1.5 py-0.5 rounded border bg-transparent cursor-pointer transition-colors focus:outline-none disabled:opacity-50',
                  DDD_ROLE_COLOR[et.ddd_role || 'entity'],
                  'border-slate-700 hover:border-slate-500',
                )}
              >
                {DDD_ROLES.map(r => (
                  <option key={r} value={r} className="bg-slate-900 text-slate-200">{DDD_ROLE_LABEL[r]}</option>
                ))}
              </select>
            </div>
          </div>
        </div>
        <div className="flex gap-2 flex-shrink-0">
          {/* Status actions */}
          {statusCfg.nextActions.map(action => (
            <button
              key={action.next}
              onClick={() => handleSetStatus(action.next)}
              disabled={statusLoading}
              className={cn(
                'px-3 py-1.5 text-xs border rounded-lg transition-colors disabled:opacity-50',
                action.next === 'deprecated'
                  ? 'border-amber-700/50 text-amber-400 hover:bg-amber-900/30'
                  : 'border-green-700/50 text-green-400 hover:bg-green-900/30',
              )}
            >
              {action.label}
            </button>
          ))}
          <button
            onClick={() => setShowAddField(true)}
            disabled={et.status === 'deprecated'}
            className="px-3 py-1.5 text-xs bg-indigo-600 hover:bg-indigo-500 text-white rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
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

      {/* P1b: Lineage Panel */}
      <LineagePanel etId={et.id} />

      {/* P2c: Interfaces Panel */}
      <InterfacesPanel etId={et.id} />

      <AddFieldDialog
        open={showAddField}
        etId={et.id}
        onClose={() => setShowAddField(false)}
        onAdded={refreshET}
      />
    </div>
  )
}
