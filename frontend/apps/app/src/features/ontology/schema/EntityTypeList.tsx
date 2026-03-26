import { useState } from 'react'
import { useSchemaStore } from '@/store/schemaStore'
import CreateETDialog from './CreateETDialog'
import { cn } from '@/lib/utils'

const DDD_ROLE_BADGE: Record<string, string> = {
  aggregate_root: 'AR',
  value_object:   'VO',
  domain_event:   'DE',
  service:        'SV',
}

export default function EntityTypeList() {
  const { entityTypes, selectedId, select } = useSchemaStore()
  const [showCreate, setShowCreate] = useState(false)

  return (
    <>
      <div className="flex items-center justify-between px-3 py-2.5 border-b border-slate-800 flex-shrink-0">
        <p className="text-[11px] font-semibold text-slate-400 uppercase tracking-wider">实体类型</p>
        <button
          onClick={() => setShowCreate(true)}
          className="text-xs px-2.5 py-1 bg-indigo-600 hover:bg-indigo-500 text-white rounded-lg transition-colors"
        >
          + 新建
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
        {entityTypes.map(et => (
          <button
            key={et.id}
            onClick={() => select(et.id)}
            className={cn(
              'w-full text-left px-2.5 py-2 rounded-lg flex items-center gap-2.5 transition-colors group',
              selectedId === et.id
                ? 'bg-indigo-900/40 border border-indigo-700/40'
                : 'hover:bg-slate-800/60',
            )}
          >
            <span className="text-base flex-shrink-0" style={{ color: et.color }}>
              {et.icon || '●'}
            </span>
            <div className="min-w-0 flex-1">
              <p className="text-sm text-slate-200 truncate font-medium">
                {et.display_name || et.name}
              </p>
              {et.namespace && (
                <p className="text-[10px] text-slate-600 truncate">{et.namespace}</p>
              )}
            </div>
            {et.ddd_role && et.ddd_role !== 'entity' && (
              <span className="text-[9px] px-1 py-0.5 rounded bg-slate-800 text-slate-500 flex-shrink-0">
                {DDD_ROLE_BADGE[et.ddd_role] ?? et.ddd_role}
              </span>
            )}
            <span className="text-[10px] text-slate-600 flex-shrink-0">
              {et.fields.length}
            </span>
          </button>
        ))}

        {entityTypes.length === 0 && (
          <p className="text-xs text-slate-600 text-center py-8">暂无实体类型</p>
        )}
      </div>

      <CreateETDialog open={showCreate} onClose={() => setShowCreate(false)} />
    </>
  )
}
