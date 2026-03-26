import { useEffect } from 'react'
import { useImportStore } from '@/store/importStore'
import { datasetsApi, entityTypesApi } from '@/api'
import DatasetList from './DatasetList'
import PromotePanel from './PromotePanel'
import type { Dataset } from '@/api/types'

export default function ImportTab() {
  const {
    datasets, selectedId,
    setDatasets, setEntityTypes, selectDataset,
  } = useImportStore()

  async function load() {
    const [ds, ets] = await Promise.all([
      datasetsApi.list(),
      entityTypesApi.list(),
    ])
    setDatasets(ds)
    setEntityTypes(ets)
    if (!selectedId && ds.length) selectDataset(ds[0].id)
  }

  useEffect(() => { load() }, [])

  const selectedDs = datasets.find(d => d.id === selectedId)

  function handleSelect(ds: Dataset) {
    selectDataset(ds.id)
  }

  return (
    <div className="flex h-full overflow-hidden">
      {/* Left: Dataset list */}
      <div className="w-72 flex-shrink-0 border-r border-slate-800 flex flex-col overflow-hidden">
        <div className="flex items-center justify-between px-4 py-3 border-b border-slate-800 flex-shrink-0">
          <div>
            <p className="text-sm font-semibold text-white">Raw Datasets</p>
            <p className="text-xs text-slate-500 mt-0.5">Select a dataset to promote</p>
          </div>
          <button
            onClick={load}
            className="text-slate-500 hover:text-slate-300 transition-colors text-sm"
            title="刷新"
          >↻</button>
        </div>
        <div className="flex-1 overflow-y-auto p-3">
          <DatasetList onSelect={handleSelect} />
        </div>
      </div>

      {/* Right: Promote panel */}
      <div className="flex-1 overflow-y-auto">
        {selectedDs ? (
          <PromotePanel key={selectedDs.id} dataset={selectedDs} />
        ) : (
          <div className="flex flex-col items-center justify-center h-full text-center px-8">
            <div className="text-5xl mb-4 text-slate-700">⬆</div>
            <h2 className="text-base font-semibold text-slate-300 mb-2">Promote a Dataset</h2>
            <p className="text-sm text-slate-500 max-w-sm">
              从左侧选择一个 Dataset，配置列映射后 Promote 到 Ontology。
            </p>
          </div>
        )}
      </div>
    </div>
  )
}
