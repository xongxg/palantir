import { useEffect, useState } from 'react'
import { entityTypesApi } from '@/api'
import { useSchemaStore } from '@/store/schemaStore'
import EntityTypeList from './EntityTypeList'
import EntityTypeDetail from './EntityTypeDetail'
import BcInferencePanel from './BcInferencePanel'

interface Props { projectId?: string }

export default function SchemaTab({ projectId }: Props) {
  const { setEntityTypes, select, selectedId } = useSchemaStore()
  const [showInfer, setShowInfer] = useState(false)

  function reload() {
    entityTypesApi.list().then(ets => {
      setEntityTypes(ets)
      if (!selectedId && ets.length) select(ets[0].id)
    }).catch(console.error)
  }

  useEffect(() => { reload() }, [])

  return (
    <div className="flex h-full overflow-hidden">
      {/* Left: ET list + BC inference toggle */}
      <div className="w-60 flex-shrink-0 border-r border-slate-800 flex flex-col overflow-hidden">
        <div className="flex-1 overflow-y-auto min-h-0">
          <EntityTypeList />
        </div>

        {projectId && (
          <>
            <button
              onClick={() => setShowInfer(v => !v)}
              className="mx-3 mb-2 mt-1 px-3 py-1.5 text-xs border border-slate-700 text-slate-500 hover:text-indigo-400 hover:border-indigo-700 rounded-lg transition-colors flex items-center justify-center gap-1.5"
            >
              <span>⬡</span>
              <span>BC 推断</span>
              <span className="ml-auto">{showInfer ? '▲' : '▼'}</span>
            </button>

            {showInfer && (
              <div className="overflow-y-auto border-t border-slate-800 pb-3">
                <BcInferencePanel projectId={projectId} onApplied={reload} />
              </div>
            )}
          </>
        )}
      </div>

      {/* Right: ET detail */}
      <div className="flex-1 overflow-y-auto">
        <EntityTypeDetail />
      </div>
    </div>
  )
}
