import { useImportStore } from '@/store/importStore'
import { cn } from '@/lib/utils'
import type { Dataset, EntityType } from '@/api/types'

interface Props {
  onSelect: (ds: Dataset) => void
}

function StatusDot({ ds }: { ds: Dataset }) {
  const promoted = ds.entity_type_id != null
  const hasRecs  = (ds.record_count || 0) > 0
  return (
    <span
      className={cn(
        'w-2 h-2 rounded-full flex-shrink-0 mt-1',
        promoted ? 'bg-emerald-400' : hasRecs ? 'bg-amber-400' : 'bg-slate-600',
      )}
    />
  )
}

function DsItem({ ds, indented, active, onSelect }: {
  ds: Dataset; indented: boolean; active: boolean; onSelect: () => void
}) {
  const promoted = ds.entity_type_id != null
  const statusText = promoted ? 'Promoted' : (ds.record_count || 0) > 0 ? 'Raw data' : 'Empty'
  const tooltip = [
    ds.fold_name  ? `Fold: ${ds.fold_name}`         : '',
    ds.source_name ? `数据源: ${ds.source_name}` : '',
  ].filter(Boolean).join('  ·  ')

  return (
    <button
      title={tooltip || undefined}
      onClick={onSelect}
      className={cn(
        'w-full text-left hover:bg-slate-700/40 transition-colors rounded-lg',
        indented ? 'pl-7 pr-3 py-1.5' : 'px-3 py-2.5',
        active && 'bg-indigo-900/30 border border-indigo-700/40',
      )}
    >
      <div className="flex items-start gap-2">
        <StatusDot ds={ds} />
        <div className="min-w-0 flex-1">
          <p className={cn('truncate font-medium', indented ? 'text-xs text-slate-300' : 'text-sm text-slate-200')}>
            {ds.name || ds.id}
          </p>
          <p className="text-xs text-slate-500 mt-0.5">{ds.record_count ?? 0} records · {statusText}</p>
          {tooltip && (
            <p className="text-xs text-slate-600 mt-0.5 truncate">{tooltip}</p>
          )}
        </div>
      </div>
    </button>
  )
}

function GroupHeader({ etName, count, totalRecords, expanded, active, onToggle }: {
  etName: string; count: number; totalRecords: number
  expanded: boolean; active: boolean; onToggle: () => void
}) {
  return (
    <div
      className={cn(
        'rounded-lg overflow-hidden',
        active && 'border border-indigo-800/40',
      )}
    >
      <button
        onClick={onToggle}
        className="w-full text-left px-3 py-2.5 hover:bg-slate-700/40 transition-colors"
      >
        <div className="flex items-center gap-2">
          <span className="w-2 h-2 rounded-full bg-emerald-400 flex-shrink-0 mt-0.5" />
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5">
              <p className="text-sm font-medium text-slate-200 truncate">{etName}</p>
              <span className="text-[9px] px-1.5 py-0.5 rounded bg-blue-950 text-blue-400 flex-shrink-0">
                {count} 个数据源
              </span>
            </div>
            <p className="text-xs text-slate-500 mt-0.5">{totalRecords} records · Promoted</p>
          </div>
          <span className="text-slate-600 text-xs flex-shrink-0">{expanded ? '▾' : '▸'}</span>
        </div>
      </button>
    </div>
  )
}

export default function DatasetList({ onSelect }: Props) {
  const { datasets, entityTypes, selectedId, expandedGroups, toggleGroup } = useImportStore()

  // Group by entity_type_id
  const etMap = Object.fromEntries(entityTypes.map(e => [e.id, e] as const))
  const grouped: Map<string, Dataset[]> = new Map()
  const singletons: Dataset[] = []
  const seenEt = new Set<string>()

  datasets.forEach(ds => {
    if (!ds.entity_type_id) { singletons.push(ds); return }
    if (!grouped.has(ds.entity_type_id)) grouped.set(ds.entity_type_id, [])
    grouped.get(ds.entity_type_id)!.push(ds)
  })

  // Build ordered list
  const items: Array<{ type: 'single'; ds: Dataset } | { type: 'group'; etId: string; items: Dataset[] }> = []
  datasets.forEach(ds => {
    if (!ds.entity_type_id) { items.push({ type: 'single', ds }); return }
    if (seenEt.has(ds.entity_type_id)) return
    seenEt.add(ds.entity_type_id)
    const group = grouped.get(ds.entity_type_id)!
    if (group.length === 1) items.push({ type: 'single', ds: group[0] })
    else items.push({ type: 'group', etId: ds.entity_type_id, items: group })
  })

  if (!datasets.length) {
    return (
      <div className="p-4 text-center">
        <p className="text-slate-400 text-sm mb-1">No datasets found</p>
        <p className="text-slate-600 text-xs">请先在 Workspace 中同步数据源</p>
      </div>
    )
  }

  return (
    <div className="space-y-1">
      {items.map((item) => {
        if (item.type === 'single') {
          return (
            <DsItem
              key={item.ds.id}
              ds={item.ds}
              indented={false}
              active={selectedId === item.ds.id}
              onSelect={() => onSelect(item.ds)}
            />
          )
        }
        const { etId, items: group } = item
        const et = etMap[etId] as EntityType | undefined
        const etName = et?.display_name || et?.name || etId
        const expanded = expandedGroups.has(etId)
        const totalRecords = group.reduce((s, d) => s + (d.record_count || 0), 0)
        const active = group.some(d => d.id === selectedId)
        return (
          <div key={etId} className={cn('rounded-lg overflow-hidden', active && 'bg-slate-900/60')}>
            <GroupHeader
              etName={etName} count={group.length}
              totalRecords={totalRecords} expanded={expanded} active={active}
              onToggle={() => toggleGroup(etId)}
            />
            {expanded && (
              <div className="bg-slate-950/60 border-t border-slate-800">
                {group.map(ds => (
                  <DsItem
                    key={ds.id} ds={ds} indented active={selectedId === ds.id}
                    onSelect={() => onSelect(ds)}
                  />
                ))}
              </div>
            )}
          </div>
        )
      })}
    </div>
  )
}
