import { useEffect, useRef, useState } from 'react'
import { toast } from 'sonner'
import { objectsApi, linksApi, entityTypesApi } from '@/api'
import type { OntologyObject, EntityType } from '@/api'
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
  onDeleted: () => void
  onNavigate: (id: string, etId: string) => void
  onLinkAdded: () => void
}
function ObjDetail({ obj, et, allObjects, onDeleted, onNavigate, onLinkAdded }: ObjDetailProps) {
  const [showAddLink, setShowAddLink] = useState(false)

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
      onLinkAdded() // refresh
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

  return (
    <div className="space-y-4 max-w-2xl">
      {/* Header */}
      <div className="flex items-start justify-between">
        <div>
          <h2 className="text-xl font-bold text-white">{obj.label}</h2>
          <p className="text-xs mt-1.5">
            <span className="px-2 py-0.5 rounded text-xs font-medium" style={{ background: (et?.color || '#6366f1') + '22', color: et?.color || '#6366f1' }}>
              {obj.entity_type_name}
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

      {/* Links */}
      <div className="bg-slate-900 border border-slate-800 rounded-lg overflow-hidden">
        <p className="text-xs font-semibold text-slate-500 uppercase tracking-wide px-4 pt-4 pb-2">
          关联关系
          {links.length > 0 && <span className="ml-2 text-slate-600 normal-case font-normal">{links.length} 条</span>}
        </p>
        {!links.length ? (
          <p className="text-slate-600 text-sm px-4 pb-4">暂无关联关系</p>
        ) : (() => {
          const outgoing = links.filter(l => l.from_id === obj.id)
          const incoming = links.filter(l => l.from_id !== obj.id)
          const renderLink = (l: typeof links[0], dir: 'out' | 'in') => {
            const otherId    = dir === 'out' ? l.to_id   : l.from_id
            const otherLabel = l.other_label  || otherId
            const otherEtName= l.other_et_name || '?'
            const otherEtId  = l.other_et_id   || ''
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
            <div className="divide-y divide-slate-800/40 pb-2">
              {outgoing.length > 0 && (
                <div>
                  <p className="text-[10px] text-indigo-400/70 font-semibold px-4 py-1.5 bg-slate-800/30 uppercase tracking-wider">
                    → 指向 ({outgoing.length})
                  </p>
                  {outgoing.map(l => renderLink(l, 'out'))}
                </div>
              )}
              {incoming.length > 0 && (
                <div>
                  <p className="text-[10px] text-emerald-500/70 font-semibold px-4 py-1.5 bg-slate-800/30 uppercase tracking-wider">
                    ← 被引用 ({incoming.length})
                  </p>
                  {incoming.map(l => renderLink(l, 'in'))}
                </div>
              )}
            </div>
          )
        })()}
      </div>

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

// ── Main Browse Tab ────────────────────────────────────────────────────────
export default function BrowseTab() {
  const [entityTypes,   setEntityTypes]   = useState<EntityType[]>([])
  const [allObjects,    setAllObjects]    = useState<OntologyObject[]>([])
  const [selectedObj,   setSelectedObj]   = useState<OntologyObject | null>(null)
  const [filterEtId,    setFilterEtId]    = useState<string | null>(null)
  const [search,        setSearch]        = useState('')
  const [showCreate,    setShowCreate]    = useState(false)
  const [loading,       setLoading]       = useState(false)
  // When navigating to a specific object, suppress auto-select of first item
  const pendingNavId = useRef<string | null>(null)

  // Load entity types once
  useEffect(() => {
    entityTypesApi.list().then(setEntityTypes).catch(console.error)
  }, [])

  // Reload objects when filter changes
  useEffect(() => {
    loadObjects()
  }, [filterEtId])

  async function loadObjects(autoSelect = true) {
    setLoading(true)
    try {
      const objs = await objectsApi.list(filterEtId ?? undefined)
      setAllObjects(objs)
      // Auto-select first object unless navigating to a specific one
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
      // Switch filter and load — suppress auto-select so we can pick the right object
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

  function handleDeleted() {
    setSelectedObj(null)
    loadObjects()
  }

  async function handleLinkAdded() {
    if (selectedObj) {
      const full = await objectsApi.get(selectedObj.id)
      setSelectedObj(full)
    }
  }

  const et = entityTypes.find(e => e.id === selectedObj?.entity_type_id)

  return (
    <div className="flex h-full overflow-hidden">
      {/* Col 1: Type filter */}
      <div className="w-44 flex-shrink-0 border-r border-slate-800 flex flex-col overflow-hidden">
        <div className="px-3 py-2.5 border-b border-slate-800 flex-shrink-0">
          <p className="text-[11px] font-semibold text-slate-400 uppercase tracking-wider">按类型筛选</p>
        </div>
        <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
          <button
            onClick={() => setFilterEtId(null)}
            className={cn(
              'w-full text-left px-2.5 py-2 rounded-lg text-sm transition-colors',
              filterEtId === null ? 'bg-indigo-900/40 text-white' : 'text-slate-300 hover:bg-slate-700/40',
            )}
          >全部对象</button>
          {entityTypes.filter(et => et.display_name !== '未分类' && et.name !== 'uncategorized').map(et => (
            <button
              key={et.id}
              onClick={() => setFilterEtId(et.id)}
              className={cn(
                'w-full text-left px-2.5 py-2 rounded-lg text-sm transition-colors flex items-center gap-2',
                filterEtId === et.id ? 'bg-indigo-900/40 text-white' : 'text-slate-300 hover:bg-slate-700/40',
              )}
            >
              <span style={{ color: et.color }} className="flex-shrink-0">{et.icon || '●'}</span>
              <span className="truncate">{et.display_name || et.name}</span>
            </button>
          ))}
        </div>
        <div className="p-2 border-t border-slate-800 flex flex-col gap-1.5 flex-shrink-0">
          <button onClick={() => setShowCreate(true)}
            className="w-full text-xs px-2.5 py-1.5 border border-slate-700 text-slate-400 hover:text-slate-200 rounded-lg transition-colors">
            + 新建对象
          </button>
        </div>
      </div>

      {/* Col 2: Object list */}
      <div className="w-64 flex-shrink-0 border-r border-slate-800 flex flex-col overflow-hidden">
        <div className="flex items-center justify-between px-3 py-2.5 border-b border-slate-800 flex-shrink-0">
          <p className="text-[11px] font-semibold text-slate-400 uppercase tracking-wider">
            {filterEtId ? (entityTypes.find(e => e.id === filterEtId)?.display_name ?? '对象') : '全部对象'}
          </p>
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
          {!loading && !allObjects.length && (
            <p className="text-xs text-slate-600 text-center py-6">暂无对象</p>
          )}
          {allObjects.filter(obj =>
            !search.trim() || obj.label.toLowerCase().includes(search.toLowerCase()) ||
            (obj.entity_type_name || '').toLowerCase().includes(search.toLowerCase())
          ).map(obj => {
            const objEt = entityTypes.find(e => e.id === obj.entity_type_id)
            return (
              <button
                key={obj.id}
                onClick={() => selectObject(obj)}
                className={cn(
                  'w-full text-left px-3 py-2 rounded-lg transition-colors',
                  selectedObj?.id === obj.id ? 'bg-indigo-900/40' : 'hover:bg-slate-800/60',
                )}
              >
                <div className="flex items-center gap-2">
                  <span style={{ color: objEt?.color || '#6366f1' }} className="flex-shrink-0">
                    {objEt?.icon || '●'}
                  </span>
                  <span className="text-sm text-white truncate">{obj.label}</span>
                </div>
                <p className="text-xs text-slate-500 mt-0.5 pl-5">{obj.entity_type_name}</p>
              </button>
            )
          })}
        </div>
      </div>

      {/* Col 3: Object detail */}
      <div className="flex-1 overflow-y-auto p-5">
        {!selectedObj ? (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <div className="text-4xl mb-3">🔍</div>
            <p className="text-slate-400 text-sm">选择对象查看详情</p>
          </div>
        ) : (
          <ObjDetail
            key={selectedObj.id}
            obj={selectedObj}
            et={et}
            allObjects={allObjects}
            onDeleted={handleDeleted}
            onNavigate={navigateTo}
            onLinkAdded={handleLinkAdded}
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
