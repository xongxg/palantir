import { useSearchParams } from 'react-router-dom'
import { cn } from '@/lib/utils'
import ImportTab from '@/features/ontology/import/ImportTab'
import SchemaTab from '@/features/ontology/schema/SchemaTab'

const TABS = ['import', 'schema', 'browse', 'graph'] as const
type Tab = typeof TABS[number]
const TAB_LABELS: Record<Tab, string> = {
  import: '导入', schema: '模型', browse: '浏览', graph: '图谱',
}

function PlaceholderTab({ name }: { name: string }) {
  return (
    <div className="flex items-center justify-center h-full">
      <p className="text-slate-600 text-sm">{name} — 迁移中…</p>
    </div>
  )
}

export default function OntologyPage() {
  const [params, setParams] = useSearchParams()
  const tab = (params.get('tab') ?? 'import') as Tab

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Tab bar */}
      <div className="flex items-center gap-1 px-4 py-2 border-b border-slate-800 flex-shrink-0">
        {TABS.map(t => (
          <button
            key={t}
            onClick={() => setParams({ tab: t })}
            className={cn(
              'px-4 py-1.5 rounded-lg text-xs font-medium transition-colors',
              tab === t
                ? 'bg-indigo-600 text-white'
                : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800',
            )}
          >
            {TAB_LABELS[t]}
          </button>
        ))}
      </div>

      {/* Tab content */}
      <div className="flex-1 overflow-hidden">
        {tab === 'import' && <ImportTab />}
        {tab === 'schema' && <SchemaTab />}
        {tab === 'browse' && <PlaceholderTab name="对象浏览" />}
        {tab === 'graph'  && <PlaceholderTab name="图谱可视化" />}
      </div>
    </div>
  )
}
