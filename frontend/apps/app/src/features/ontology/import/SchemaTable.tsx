import { useImportStore } from '@/store/importStore'
import { cn } from '@/lib/utils'

const DATA_TYPES = ['string', 'number', 'boolean', 'date', 'datetime', 'reference']

interface Props {
  savedPk: string
  onPkChange: (col: string) => void
}

export default function SchemaTable({ savedPk, onPkChange }: Props) {
  const { columns, updateColumn } = useImportStore()

  if (!columns.length) {
    return (
      <tr>
        <td colSpan={5} className="text-center py-6 text-slate-500 text-xs">
          选择 dataset 后自动识别
        </td>
      </tr>
    )
  }

  return (
    <>
      {columns.map(col => (
        <tr key={col.name} className={cn('border-b border-slate-800/60', col.ignored && 'opacity-40')}>
          {/* Column name (editable) */}
          <td className="py-2 px-2">
            <input
              value={col.editedName}
              onChange={e => updateColumn(col.name, { editedName: e.target.value })}
              className="bg-slate-900 border border-slate-700 rounded px-2 py-1 text-xs text-slate-200 w-full focus:outline-none focus:border-indigo-500"
            />
          </td>
          {/* Type */}
          <td className="py-2 px-2">
            <select
              value={col.inferredType}
              onChange={e => updateColumn(col.name, { inferredType: e.target.value })}
              className="bg-slate-900 border border-slate-700 rounded px-2 py-1 text-xs text-slate-200 w-full focus:outline-none focus:border-indigo-500"
            >
              {DATA_TYPES.map(t => (
                <option key={t} value={t}>{t}</option>
              ))}
            </select>
          </td>
          {/* Sample values */}
          <td className="py-2 px-2">
            <span className="text-xs text-slate-500">
              {col.samples.slice(0, 2).join(' / ') || '—'}
            </span>
          </td>
          {/* Primary key radio */}
          <td className="py-2 px-2 text-center">
            <input
              type="radio"
              name="pk-col"
              value={col.name}
              checked={(savedPk || '') === col.name}
              onChange={() => onPkChange(col.name)}
              className="accent-indigo-500"
            />
          </td>
          {/* Ignore checkbox */}
          <td className="py-2 px-2 text-center">
            <input
              type="checkbox"
              checked={col.ignored}
              onChange={e => updateColumn(col.name, { ignored: e.target.checked })}
              className="accent-indigo-500"
            />
          </td>
        </tr>
      ))}
    </>
  )
}
