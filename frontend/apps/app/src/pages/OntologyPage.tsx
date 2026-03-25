import { useSearchParams } from 'react-router-dom'
import { cn } from '@/lib/utils'

const TABS = ['import', 'schema', 'browse', 'graph'] as const
type Tab = typeof TABS[number]

const TAB_LABELS: Record<Tab, string> = {
  import: '导入',
  schema: '模型',
  browse: '浏览',
  graph:  '图谱',
}

export default function OntologyPage() {
  const [params, setParams] = useSearchParams()
  const tab = (params.get('tab') ?? 'import') as Tab

  return (
    <div className="flex h-full">
      {/* TODO: migrate import/schema/browse/graph tabs from ontology.html */}
      <div className="flex-1 flex items-center justify-center">
        <div className="text-center">
          <p className="text-slate-400 text-sm">
            当前 Tab：<span className="text-indigo-400 font-medium">{TAB_LABELS[tab] ?? tab}</span>
          </p>
          <p className="text-slate-600 text-xs mt-2">迁移中…</p>
          <div className="mt-6 flex gap-2 justify-center">
            {TABS.map(t => (
              <button
                key={t}
                onClick={() => setParams({ tab: t })}
                className={cn(
                  'px-3 py-1.5 rounded text-xs font-medium transition-colors',
                  tab === t
                    ? 'bg-indigo-600 text-white'
                    : 'bg-slate-800 text-slate-400 hover:text-slate-200',
                )}
              >
                {TAB_LABELS[t]}
              </button>
            ))}
          </div>
        </div>
      </div>
    </div>
  )
}
