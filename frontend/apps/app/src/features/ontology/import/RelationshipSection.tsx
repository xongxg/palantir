import { useImportStore } from '@/store/importStore'
import { cn } from '@/lib/utils'

interface Props { entityTypeId: string }

const REL_TYPES = ['HAS', 'BELONGS_TO', 'REFERENCES', 'LINKS_TO']

export default function RelationshipSection({ entityTypeId }: Props) {
  const { columns, entityTypes, links, addLink, removeLink, updateLink } = useImportStore()

  const fkCols = columns.filter(c => !c.ignored).map(c => c.editedName || c.name)

  return (
    <div className="space-y-2">
      {links.map((link, idx) => (
        <div key={idx} className="flex items-center gap-2">
          {/* FK column */}
          <select
            value={link.fkCol}
            onChange={e => updateLink(idx, { fkCol: e.target.value })}
            className="flex-1 bg-slate-900 border border-slate-700 rounded px-2 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
          >
            <option value="">— FK 列 —</option>
            {fkCols.map(c => <option key={c} value={c}>{c}</option>)}
          </select>

          <span className="text-slate-600 text-xs flex-shrink-0">→</span>

          {/* Target ET */}
          <select
            value={link.toEntityTypeId}
            onChange={e => updateLink(idx, { toEntityTypeId: e.target.value })}
            className="flex-1 bg-slate-900 border border-slate-700 rounded px-2 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
          >
            <option value="">— 目标类型 —</option>
            {entityTypes
              .filter(et => et.id !== entityTypeId)
              .map(et => (
                <option key={et.id} value={et.id}>
                  {et.namespace ? `[${et.namespace}] ` : ''}{et.display_name || et.name}
                </option>
              ))}
          </select>

          {/* Rel type */}
          <select
            value={link.relType}
            onChange={e => updateLink(idx, { relType: e.target.value })}
            className="w-28 bg-slate-900 border border-slate-700 rounded px-2 py-1.5 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
          >
            {REL_TYPES.map(r => <option key={r} value={r}>{r}</option>)}
          </select>

          {/* Remove */}
          <button
            onClick={() => removeLink(idx)}
            className="w-6 h-6 flex items-center justify-center rounded-full bg-red-900/40 text-red-400 hover:bg-red-800/60 transition-colors flex-shrink-0 text-xs"
          >✕</button>
        </div>
      ))}

      {links.length === 0 && (
        <p className="text-xs text-slate-600 py-1">选择 dataset 后自动推导…</p>
      )}

      <button
        onClick={() => addLink({ fkCol: '', toEntityTypeId: '', relType: 'HAS' })}
        className={cn(
          'text-xs text-slate-400 hover:text-slate-200 transition-colors',
          links.length > 0 && 'mt-1',
        )}
      >
        + 手动添加
      </button>
    </div>
  )
}
