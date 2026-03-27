import { useEffect, useRef, useState, useMemo } from 'react'
import { toast } from 'sonner'
import { objectsApi, linksApi, entityTypesApi, graphApi } from '@/api'
import type { OntologyObject, EntityType, GraphNode, GraphEdge } from '@/api'
import { cn } from '@/lib/utils'

// ── Create Object Dialog ───────────────────────────────────────────────────
interface CreateObjProps {
  entityTypes: EntityType[]
  onClose: () => void
  onCreated: () => void
}
function CreateObjectDialog({ entityTypes, onClose, onCreated }: CreateObjProps) {
  const [etId,    setEtId]    = useState(entityTypes[0]?.id ?? '')
  const [label,   setLabel]   = useState('')
  const [fields,  setFields]  = useState<Record<string, string>>({})
  const [loading, setLoading] = useState(false)

  const et = entityTypes.find(e => e.id === etId)

  async function handleSubmit() {
    if (!label.trim() || !etId) { toast.error('请填写标签和类型'); return }
    setLoading(true)
    try {
      const parsedFields: Record<string, unknown> = {}
      Object.entries(fields).forEach(([k, v]) => { if (v.trim()) parsedFields[k] = v.trim() })
      await objectsApi.create({ entity_type_id: etId, label: label.trim(), fields: parsedFields })
      toast.success('对象已创建')
      onCreated()
      onClose()
    } catch (e) {
      toast.error(String(e))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 w-96 space-y-4 shadow-2xl max-h-[80vh] overflow-y-auto">
        <h3 className="text-base font-semibold text-slate-100">新建对象</h3>
        <div className="space-y-3">
          <label className="block">
            <span className="text-xs text-slate-400 mb-1 block">实体类型</span>
            <select value={etId} onChange={e => setEtId(e.target.value)}
              className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500">
              {entityTypes.map(e => <option key={e.id} value={e.id}>{e.display_name || e.name}</option>)}
            </select>
          </label>
          <label className="block">
            <span className="text-xs text-slate-400 mb-1 block">标签</span>
            <input value={label} onChange={e => setLabel(e.target.value)}
              placeholder="对象名称"
              onKeyDown={e => e.key === 'Enter' && handleSubmit()}
              className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500"
            />
          </label>
          {et?.fields.map(f => (
            <label key={f.id} className="block">
              <span className="text-xs text-slate-400 mb-1 block">{f.name}{f.is_required && <span className="text-red-400 ml-1">*</span>}</span>
              <input
                value={fields[f.name] ?? ''}
                onChange={e => setFields(prev => ({ ...prev, [f.name]: e.target.value }))}
                className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500"
              />
            </label>
          ))}
        </div>
        <div className="flex gap-2 justify-end pt-1">
          <button onClick={onClose} className="px-4 py-2 text-sm text-slate-400 hover:text-slate-200 border border-slate-700 rounded-lg transition-colors">取消</button>
          <button onClick={handleSubmit} disabled={loading}
            className="px-4 py-2 text-sm bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white rounded-lg transition-colors">
            {loading ? '创建中…' : '创建'}
          </button>
        </div>
      </div>
    </div>
  )
}

// ── Add Link Dialog ────────────────────────────────────────────────────────
interface AddLinkProps {
  fromObj: OntologyObject
  allObjects: OntologyObject[]
  onClose: () => void
  onAdded: () => void
}
function AddLinkDialog({ fromObj, allObjects, onClose, onAdded }: AddLinkProps) {
  const [relType, setRelType] = useState('')
  const [toId,    setToId]    = useState('')
  const [loading, setLoading] = useState(false)

  const candidates = allObjects.filter(o => o.id !== fromObj.id)

  async function handleSubmit() {
    if (!relType.trim() || !toId) { toast.error('请填写关系类型和目标对象'); return }
    setLoading(true)
    try {
      await linksApi.create({ from_id: fromObj.id, to_id: toId, rel_type: relType.trim() })
      toast.success('关联已建立')
      onAdded()
      onClose()
    } catch (e) {
      toast.error(String(e))
    } finally {
      setLoading(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="bg-slate-900 border border-slate-700 rounded-xl p-6 w-80 space-y-4 shadow-2xl">
        <h3 className="text-base font-semibold text-slate-100">建立关联</h3>
        <div className="space-y-3">
          <label className="block">
            <span className="text-xs text-slate-400 mb-1 block">关系类型</span>
            <input value={relType} onChange={e => setRelType(e.target.value)}
              placeholder="HAS / BELONGS_TO / ..."
              autoFocus
              onKeyDown={e => e.key === 'Enter' && handleSubmit()}
              className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500"
            />
          </label>
          <label className="block">
            <span className="text-xs text-slate-400 mb-1 block">目标对象</span>
            <select value={toId} onChange={e => setToId(e.target.value)}
              className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500">
              <option value="">— 选择对象 —</option>
              {candidates.map(o => (
                <option key={o.id} value={o.id}>{o.label} ({o.entity_type_name})</option>
              ))}
            </select>
          </label>
        </div>
        <div className="flex gap-2 justify-end pt-1">
          <button onClick={onClose} className="px-4 py-2 text-sm text-slate-400 hover:text-slate-200 border border-slate-700 rounded-lg transition-colors">取消</button>
          <button onClick={handleSubmit} disabled={loading}
            className="px-4 py-2 text-sm bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white rounded-lg transition-colors">
            {loading ? '建立中…' : '确定'}
          </button>
        </div>
      </div>
    </div>
  )
}

// ── Object Detail pane ─────────────────────────────────────────────────────
interface ObjDetailProps {
  obj: OntologyObject
  et: EntityType | undefined
  allObjects: OntologyObject[]
  entityTypes: EntityType[]
  childToAR: Map<string, string>   // childEtId → arEtId
  onDeleted: () => void
  onNavigate: (id: string, etId: string) => void
  onLinkAdded: () => void
  onSelectEt: (id: string) => void
}
function ObjDetail({ obj, et, allObjects, entityTypes, childToAR, onDeleted, onNavigate, onLinkAdded, onSelectEt }: ObjDetailProps) {
  const [showAddLink, setShowAddLink] = useState(false)

  const isAR = et?.ddd_role === 'aggregate_root'
  const parentARId = et ? childToAR.get(et.id) : undefined
  const parentAR   = parentARId ? entityTypes.find(e => e.id === parentARId) : undefined

  async function handleDelete() {
    if (!confirm(`确认删除「${obj.label}」？`)) return
    try {
      await objectsApi.delete(obj.id)
      toast.success('已删除')
      onDeleted()
    } catch (e) {
      toast.error(String(e))
    }
  }

  async function handleDeleteLink(linkId: string) {
    try {
      await linksApi.delete(linkId)
      toast.success('关联已删除')
      onLinkAdded()
    } catch (e) {
      toast.error(String(e))
    }
  }

  const fieldDefs = et?.fields ?? []
  const fields    = obj.fields || {}
  const entries   = fieldDefs.length
    ? fieldDefs.map(sf => [sf.name, fields[sf.name]] as [string, unknown])
    : Object.entries(fields)
  const links = obj.links ?? []

  // Split links into aggregate members (AR→child outgoing) vs other relations
  const outgoing = links.filter(l => l.from_id === obj.id)
  const incoming = links.filter(l => l.from_id !== obj.id)

  // For AR objects: outgoing links = aggregate members, grouped by ET
  const membersByET = useMemo(() => {
    if (!isAR) return new Map<string, typeof outgoing>()
    const map = new Map<string, typeof outgoing>()
    outgoing.forEach(l => {
      const etName = l.other_et_name || '?'
      if (!map.has(etName)) map.set(etName, [])
      map.get(etName)!.push(l)
    })
    return map
  }, [isAR, outgoing])

  // For non-AR: incoming links from AR objects = "归属聚合根"
  const arIncoming = incoming.filter(l => {
    const srcEt = entityTypes.find(e => e.name === l.other_et_name || e.display_name === l.other_et_name)
    return srcEt?.ddd_role === 'aggregate_root'
  })
  const otherIncoming = incoming.filter(l => !arIncoming.includes(l))

  const renderLinkRow = (l: typeof links[0], dir: 'out' | 'in') => {
    const otherId     = dir === 'out' ? l.to_id    : l.from_id
    const otherLabel  = l.other_label   || otherId
    const otherEtName = l.other_et_name || '?'
    const otherEtId   = l.other_et_id   || ''
    return (
      <div key={l.id} className="flex items-center justify-between px-4 py-2 hover:bg-slate-800/40 transition-colors group">
        <div className="flex items-center gap-2 min-w-0 flex-1">
          <span className={cn('text-[10px] font-bold flex-shrink-0 w-3', dir === 'out' ? 'text-indigo-400' : 'text-emerald-500')}>
            {dir === 'out' ? '→' : '←'}
          </span>
          <span className="text-[10px] font-mono px-1.5 py-0.5 rounded flex-shrink-0 bg-slate-800 text-slate-400">
            {l.rel_type}
          </span>
          <button
            onClick={() => onNavigate(otherId, otherEtId)}
            className="text-slate-200 hover:text-indigo-300 truncate text-sm text-left hover:underline underline-offset-2 transition-colors"
          >{otherLabel}</button>
          <span className="text-slate-600 text-[10px] flex-shrink-0 italic">{otherEtName}</span>
        </div>
        <button
          onClick={() => handleDeleteLink(l.id)}
          className="text-slate-800 group-hover:text-slate-600 hover:!text-red-400 transition-colors text-xs flex-shrink-0 ml-2 px-1"
        >×</button>
      </div>
    )
  }

  return (
    <div className="space-y-4 max-w-2xl">
      {/* Header */}
      <div className="flex items-start justify-between">
        <div>
          {/* Breadcrumb for non-AR: show parent AR */}
          {!isAR && parentAR && (
            <p className="text-xs text-slate-500 mb-1">
              <span className="text-red-400">◆</span>{' '}
              <button
                onClick={() => onSelectEt(parentAR.id)}
                className="hover:text-indigo-300 transition-colors"
              >{parentAR.display_name || parentAR.name}</button>
              <span className="mx-1">/</span>
              <span className="text-slate-400">{et?.display_name || et?.name}</span>
            </p>
          )}
          <div className="flex items-center gap-2">
            {isAR && <span className="text-red-400 text-sm">◆</span>}
            <h2 className="text-xl font-bold text-white">{obj.label}</h2>
          </div>
          <p className="text-xs mt-1.5">
            <span className="px-2 py-0.5 rounded text-xs font-medium" style={{ background: (et?.color || '#6366f1') + '22', color: et?.color || '#6366f1' }}>
              {isAR ? 'AR · ' : ''}{obj.entity_type_name}
            </span>
          </p>
        </div>
        <div className="flex gap-2 flex-shrink-0">
          <button onClick={() => setShowAddLink(true)}
            className="px-3 py-1.5 text-xs bg-indigo-600 hover:bg-indigo-500 text-white rounded-lg transition-colors">
            + 建立关联
          </button>
          <button onClick={handleDelete}
            className="px-3 py-1.5 text-xs bg-red-900/40 hover:bg-red-800/60 text-red-400 border border-red-800/40 rounded-lg transition-colors">
            删除
          </button>
        </div>
      </div>

      {/* Fields */}
      <div className="bg-slate-900 border border-slate-800 rounded-lg p-4">
        <p className="text-xs font-semibold text-slate-500 uppercase tracking-wide mb-3">字段</p>
        {!entries.length
          ? <p className="text-slate-600 text-xs">暂无字段</p>
          : <div className="space-y-1.5 text-sm divide-y divide-slate-800/60">
              {entries.map(([k, v]) => (
                <div key={k} className="flex justify-between gap-4 py-1.5">
                  <span className="text-slate-500 font-mono text-xs flex-shrink-0">{k}</span>
                  <span className="text-slate-200 text-xs text-right">
                    {v !== undefined && v !== null ? String(v) : <span className="text-slate-700">—</span>}
                  </span>
                </div>
              ))}
            </div>
        }
      </div>

      {/* AR: Aggregate Members panel */}
      {isAR && membersByET.size > 0 && (
        <div className="bg-slate-900 border border-red-900/30 rounded-lg overflow-hidden">
          <p className="text-xs font-semibold text-red-400/80 uppercase tracking-wide px-4 pt-4 pb-2 flex items-center gap-1.5">
            <span>◆</span> 聚合成员
            <span className="text-slate-600 normal-case font-normal ml-1">{outgoing.length} 个对象</span>
          </p>
          <div className="divide-y divide-slate-800/40 pb-2">
            {Array.from(membersByET.entries()).map(([etName, members]) => (
              <div key={etName}>
                <p className="text-[10px] text-slate-500 font-semibold px-4 py-1.5 bg-slate-800/30 uppercase tracking-wider">
                  {etName} · {members.length}
                </p>
                {members.map(l => renderLinkRow(l, 'out'))}
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Non-AR: Belongs-to AR panel */}
      {!isAR && arIncoming.length > 0 && (
        <div className="bg-slate-900 border border-red-900/20 rounded-lg overflow-hidden">
          <p className="text-xs font-semibold text-red-400/70 uppercase tracking-wide px-4 pt-4 pb-2 flex items-center gap-1.5">
            <span>◆</span> 归属聚合根
          </p>
          <div className="divide-y divide-slate-800/40 pb-2">
            {arIncoming.map(l => renderLinkRow(l, 'in'))}
          </div>
        </div>
      )}

      {/* Regular links (non-aggregate) */}
      {((!isAR && (otherIncoming.length > 0 || outgoing.length > 0)) ||
        (isAR && (incoming.length > 0 || (membersByET.size === 0 && outgoing.length > 0)))) && (
        <div className="bg-slate-900 border border-slate-800 rounded-lg overflow-hidden">
          <p className="text-xs font-semibold text-slate-500 uppercase tracking-wide px-4 pt-4 pb-2">
            关联关系
            {links.length > 0 && <span className="ml-2 text-slate-600 normal-case font-normal">{links.length} 条</span>}
          </p>
          {!links.length ? (
            <p className="text-slate-600 text-sm px-4 pb-4">暂无关联关系</p>
          ) : (
            <div className="divide-y divide-slate-800/40 pb-2">
              {/* AR: show incoming only (outgoing shown in members panel) */}
              {isAR && incoming.length > 0 && (
                <div>
                  <p className="text-[10px] text-emerald-500/70 font-semibold px-4 py-1.5 bg-slate-800/30 uppercase tracking-wider">
                    ← 被引用 ({incoming.length})
                  </p>
                  {incoming.map(l => renderLinkRow(l, 'in'))}
                </div>
              )}
              {/* Non-AR: show outgoing + non-AR incoming */}
              {!isAR && outgoing.length > 0 && (
                <div>
                  <p className="text-[10px] text-indigo-400/70 font-semibold px-4 py-1.5 bg-slate-800/30 uppercase tracking-wider">
                    → 指向 ({outgoing.length})
                  </p>
                  {outgoing.map(l => renderLinkRow(l, 'out'))}
                </div>
              )}
              {!isAR && otherIncoming.length > 0 && (
                <div>
                  <p className="text-[10px] text-emerald-500/70 font-semibold px-4 py-1.5 bg-slate-800/30 uppercase tracking-wider">
                    ← 被引用 ({otherIncoming.length})
                  </p>
                  {otherIncoming.map(l => renderLinkRow(l, 'in'))}
                </div>
              )}
              {/* AR with no members yet: show outgoing as plain links */}
              {isAR && membersByET.size === 0 && outgoing.length > 0 && (
                <div>
                  <p className="text-[10px] text-indigo-400/70 font-semibold px-4 py-1.5 bg-slate-800/30 uppercase tracking-wider">
                    → 指向 ({outgoing.length})
                  </p>
                  {outgoing.map(l => renderLinkRow(l, 'out'))}
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {showAddLink && (
        <AddLinkDialog
          fromObj={obj}
          allObjects={allObjects}
          onClose={() => setShowAddLink(false)}
          onAdded={onLinkAdded}
        />
      )}
    </div>
  )
}

// ── Left panel: AR-centric ET tree ────────────────────────────────────────
interface ETTreeProps {
  entityTypes: EntityType[]
  arChildMap: Map<string, string[]>   // arEtId → childEtIds[]
  filterEtId: string | null
  onSelect: (id: string | null) => void
  onRoleChanged: () => void
}
function ETTree({ entityTypes, arChildMap, filterEtId, onSelect, onRoleChanged }: ETTreeProps) {
  const [expanded, setExpanded] = useState<Set<string>>(new Set())

  const arETs      = entityTypes.filter(e => e.ddd_role === 'aggregate_root')
  const childEtIds = new Set(Array.from(arChildMap.values()).flat())
  const orphanETs  = entityTypes.filter(e =>
    e.ddd_role !== 'aggregate_root' &&
    !childEtIds.has(e.id) &&
    e.display_name !== '未分类' && e.name !== 'uncategorized'
  )

  function toggle(id: string) {
    setExpanded(prev => {
      const next = new Set(prev)
      next.has(id) ? next.delete(id) : next.add(id)
      return next
    })
  }

  // Auto-expand AR that contains the currently selected child ET
  useEffect(() => {
    if (!filterEtId) return
    arChildMap.forEach((children, arId) => {
      if (children.includes(filterEtId)) {
        setExpanded(prev => new Set([...prev, arId]))
      }
    })
  }, [filterEtId, arChildMap])

  const btnCls = (id: string | null) => cn(
    'w-full text-left px-2.5 py-1.5 rounded-lg text-xs transition-colors',
    filterEtId === id ? 'bg-indigo-900/40 text-white' : 'text-slate-300 hover:bg-slate-700/40',
  )

  return (
    <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
      {/* All */}
      <button onClick={() => onSelect(null)} className={btnCls(null)}>全部对象</button>

      {/* AR ETs + children */}
      {arETs.map(ar => {
        const children = (arChildMap.get(ar.id) || [])
          .map(id => entityTypes.find(e => e.id === id))
          .filter(Boolean) as EntityType[]
        const isOpen = expanded.has(ar.id)

        return (
          <div key={ar.id}>
            <div className="flex items-center gap-1">
              {children.length > 0 && (
                <button
                  onClick={() => toggle(ar.id)}
                  className="text-slate-600 hover:text-slate-400 w-3 flex-shrink-0 text-[10px]"
                >{isOpen ? '▾' : '▸'}</button>
              )}
              <button
                onClick={() => onSelect(ar.id)}
                className={cn(
                  btnCls(ar.id),
                  'flex items-center gap-1.5 flex-1',
                  children.length === 0 && 'pl-4',
                )}
              >
                <span className="text-red-400 text-[10px] flex-shrink-0">◆</span>
                <span style={{ color: ar.color }} className="flex-shrink-0 text-[10px]">{ar.icon || '●'}</span>
                <span className="truncate font-medium">{ar.display_name || ar.name}</span>
              </button>
            </div>

            {/* Child ETs */}
            {isOpen && children.map(child => (
              <button
                key={child.id}
                onClick={() => onSelect(child.id)}
                className={cn(btnCls(child.id), 'flex items-center gap-1.5 pl-7')}
              >
                <span style={{ color: child.color }} className="flex-shrink-0 text-[10px]">{child.icon || '●'}</span>
                <span className="truncate text-slate-400">{child.display_name || child.name}</span>
              </button>
            ))}
          </div>
        )
      })}

      {/* Orphan ETs */}
      {orphanETs.length > 0 && (
        <>
          <div className="pt-2 pb-1 px-2.5">
            <span className="text-[10px] text-slate-600 uppercase tracking-wider">其他</span>
          </div>
          {orphanETs.map(et => (
            <div key={et.id} className="flex items-center gap-1 group">
              <button onClick={() => onSelect(et.id)} className={cn(btnCls(et.id), 'flex items-center gap-1.5 flex-1')}>
                <span style={{ color: et.color }} className="flex-shrink-0 text-[10px]">{et.icon || '●'}</span>
                <span className="truncate">{et.display_name || et.name}</span>
              </button>
              <button
                title="设为 Aggregate Root"
                onClick={async () => {
                  await entityTypesApi.setDddRole(et.id, 'aggregate_root')
                  onRoleChanged()
                }}
                className="opacity-0 group-hover:opacity-100 text-slate-600 hover:text-red-400 text-[10px] px-1 flex-shrink-0 transition-all"
              >◆</button>
            </div>
          ))}
        </>
      )}
    </div>
  )
}

// ── Main Browse Tab ────────────────────────────────────────────────────────
export default function BrowseTab({ projectId }: { projectId: string }) {
  const [entityTypes,   setEntityTypes]   = useState<EntityType[]>([])
  const [allObjects,    setAllObjects]    = useState<OntologyObject[]>([])
  const [selectedObj,   setSelectedObj]   = useState<OntologyObject | null>(null)
  const [filterEtId,    setFilterEtId]    = useState<string | null>(null)
  const [search,        setSearch]        = useState('')
  const [showCreate,    setShowCreate]    = useState(false)
  const [loading,       setLoading]       = useState(false)
  const [graphNodes,    setGraphNodes]    = useState<GraphNode[]>([])
  const [graphEdges,    setGraphEdges]    = useState<GraphEdge[]>([])
  const pendingNavId = useRef<string | null>(null)

  useEffect(() => {
    entityTypesApi.list().then(setEntityTypes).catch(console.error)
    // Load graph for AR→child ET derivation
    graphApi.get(projectId).then(d => {
      setGraphNodes(d.nodes ?? [])
      setGraphEdges(d.edges ?? [])
    }).catch(console.error)
  }, [])

  useEffect(() => { loadObjects() }, [filterEtId])

  // Derive AR → child ET map from graph edges
  const arChildMap = useMemo(() => {
    const arEtIds = new Set(entityTypes.filter(e => e.ddd_role === 'aggregate_root').map(e => e.id))
    if (!arEtIds.size || !graphNodes.length) return new Map<string, string[]>()

    const nodeToET = new Map(graphNodes.map(n => [n.id, n.et_id]))
    const map = new Map<string, Set<string>>()

    graphEdges.forEach(edge => {
      const srcId = typeof edge.source === 'string' ? edge.source : (edge.source as any).id
      const tgtId = typeof edge.target === 'string' ? edge.target : (edge.target as any).id
      const srcET = nodeToET.get(srcId)
      const tgtET = nodeToET.get(tgtId)
      if (!srcET || !tgtET || srcET === tgtET) return
      if (arEtIds.has(srcET)) {
        if (!map.has(srcET)) map.set(srcET, new Set())
        map.get(srcET)!.add(tgtET)
      }
    })

    return new Map(Array.from(map.entries()).map(([k, v]) => [k, Array.from(v)]))
  }, [entityTypes, graphNodes, graphEdges])

  // child ET → parent AR ET
  const childToAR = useMemo(() => {
    const map = new Map<string, string>()
    arChildMap.forEach((children, arId) => children.forEach(c => map.set(c, arId)))
    return map
  }, [arChildMap])

  async function loadObjects(autoSelect = true) {
    setLoading(true)
    try {
      const objs = await objectsApi.list(filterEtId ?? undefined)
      setAllObjects(objs)
      if (autoSelect && !pendingNavId.current && objs.length) {
        const full = await objectsApi.get(objs[0].id)
        setSelectedObj(full)
      } else if (!objs.length) {
        setSelectedObj(null)
      }
    } catch (e) {
      toast.error(String(e))
    } finally {
      setLoading(false)
    }
  }

  async function selectObject(obj: OntologyObject) {
    try {
      const full = await objectsApi.get(obj.id)
      setSelectedObj(full)
    } catch {
      setSelectedObj(obj)
    }
  }

  async function navigateTo(objectId: string, etId: string) {
    if (etId && etId !== filterEtId) {
      pendingNavId.current = objectId
      setFilterEtId(etId)
      try {
        const objs = await objectsApi.list(etId)
        setAllObjects(objs)
        const found = objs.find(o => o.id === objectId)
        if (found) await selectObject(found)
        else {
          const full = await objectsApi.get(objectId)
          setSelectedObj(full)
        }
      } finally {
        pendingNavId.current = null
      }
    } else {
      const found = allObjects.find(o => o.id === objectId)
      if (found) await selectObject(found)
      else {
        const full = await objectsApi.get(objectId)
        setSelectedObj(full)
      }
    }
  }

  function handleDeleted() { setSelectedObj(null); loadObjects() }
  async function handleLinkAdded() {
    if (selectedObj) setSelectedObj(await objectsApi.get(selectedObj.id))
  }

  const et = entityTypes.find(e => e.id === selectedObj?.entity_type_id)

  // Header label for Col 2
  const col2Label = filterEtId
    ? (entityTypes.find(e => e.id === filterEtId)?.display_name ?? '对象')
    : '全部对象'

  const filteredObjs = allObjects.filter(obj =>
    !search.trim() ||
    obj.label.toLowerCase().includes(search.toLowerCase()) ||
    (obj.entity_type_name || '').toLowerCase().includes(search.toLowerCase())
  )

  return (
    <div className="flex h-full overflow-hidden">
      {/* Col 1: AR-centric ET tree */}
      <div className="w-48 flex-shrink-0 border-r border-slate-800 flex flex-col overflow-hidden">
        <div className="px-3 py-2.5 border-b border-slate-800 flex-shrink-0">
          <p className="text-[11px] font-semibold text-slate-400 uppercase tracking-wider">按类型筛选</p>
        </div>
        <ETTree
          entityTypes={entityTypes.filter(e => e.display_name !== '未分类' && e.name !== 'uncategorized')}
          arChildMap={arChildMap}
          filterEtId={filterEtId}
          onSelect={setFilterEtId}
          onRoleChanged={async () => {
            const ets = await entityTypesApi.list()
            setEntityTypes(ets)
            graphApi.get(projectId).then(d => {
              setGraphNodes(d.nodes ?? [])
              setGraphEdges(d.edges ?? [])
            }).catch(console.error)
          }}
        />
        <div className="p-2 border-t border-slate-800 flex-shrink-0">
          <button onClick={() => setShowCreate(true)}
            className="w-full text-xs px-2.5 py-1.5 border border-slate-700 text-slate-400 hover:text-slate-200 rounded-lg transition-colors">
            + 新建对象
          </button>
        </div>
      </div>

      {/* Col 2: Object list */}
      <div className="w-64 flex-shrink-0 border-r border-slate-800 flex flex-col overflow-hidden">
        <div className="flex items-center justify-between px-3 py-2.5 border-b border-slate-800 flex-shrink-0">
          <p className="text-[11px] font-semibold text-slate-400 uppercase tracking-wider">{col2Label}</p>
          {!loading && <span className="text-[11px] text-slate-600">{allObjects.length || ''}</span>}
        </div>
        <div className="px-2 py-1.5 border-b border-slate-800/60 flex-shrink-0">
          <input
            value={search}
            onChange={e => setSearch(e.target.value)}
            placeholder="搜索…"
            className="w-full bg-slate-900 border border-slate-700/60 rounded-md px-2.5 py-1 text-xs text-slate-200 placeholder-slate-600 focus:outline-none focus:border-indigo-600"
          />
        </div>
        <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
          {loading && <p className="text-xs text-slate-600 text-center py-6">加载中…</p>}
          {!loading && !filteredObjs.length && (
            <p className="text-xs text-slate-600 text-center py-6">暂无对象</p>
          )}
          {filteredObjs.map(obj => {
            const objEt = entityTypes.find(e => e.id === obj.entity_type_id)
            const isObjAR = objEt?.ddd_role === 'aggregate_root'
            return (
              <button
                key={obj.id}
                onClick={() => selectObject(obj)}
                className={cn(
                  'w-full text-left px-3 py-2 rounded-lg transition-colors',
                  selectedObj?.id === obj.id ? 'bg-indigo-900/40' : 'hover:bg-slate-800/60',
                )}
              >
                <div className="flex items-center gap-1.5">
                  {isObjAR && <span className="text-red-400 text-[10px] flex-shrink-0">◆</span>}
                  <span style={{ color: objEt?.color || '#6366f1' }} className="flex-shrink-0 text-[10px]">
                    {objEt?.icon || '●'}
                  </span>
                  <span className="text-sm text-white truncate">{obj.label}</span>
                </div>
                {!filterEtId && (
                  <p className="text-xs text-slate-500 mt-0.5 pl-5">{obj.entity_type_name}</p>
                )}
              </button>
            )
          })}
        </div>
      </div>

      {/* Col 3: Object detail */}
      <div className="flex-1 overflow-y-auto p-5">
        {!selectedObj ? (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <div className="text-4xl mb-3 text-red-400/40">◆</div>
            <p className="text-slate-400 text-sm">选择对象查看详情</p>
            <p className="text-slate-600 text-xs mt-1">从左侧选择 Aggregate Root 开始浏览</p>
          </div>
        ) : (
          <ObjDetail
            key={selectedObj.id}
            obj={selectedObj}
            et={et}
            allObjects={allObjects}
            entityTypes={entityTypes}
            childToAR={childToAR}
            onDeleted={handleDeleted}
            onNavigate={navigateTo}
            onLinkAdded={handleLinkAdded}
            onSelectEt={setFilterEtId}
          />
        )}
      </div>

      {showCreate && (
        <CreateObjectDialog
          entityTypes={entityTypes}
          onClose={() => setShowCreate(false)}
          onCreated={loadObjects}
        />
      )}
    </div>
  )
}
