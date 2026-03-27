import { useEffect, useState } from 'react'
import { toast } from 'sonner'
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
  const [promotingAll, setPromotingAll] = useState(false)

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

  async function handlePromoteAll() {
    setPromotingAll(true)
    let total = 0
    let failed = 0
    let skipped = 0
    for (const ds of datasets) {
      try {
        const { mapping } = await datasetsApi.getMapping(ds.id)
        if (!mapping?.entity_type_id) { skipped++; continue }
        const fm = mapping.field_mapping
          ? (typeof mapping.field_mapping === 'string'
              ? JSON.parse(mapping.field_mapping as unknown as string)
              : mapping.field_mapping)
          : undefined
        const result = await datasetsApi.promote(ds.id, {
          entity_type_id: mapping.entity_type_id,
          new_entity_type: undefined,
          sync_mode: mapping.sync_mode ?? 'snapshot',
          primary_key_col: mapping.primary_key_col ?? undefined,
          field_mapping: fm && Object.keys(fm).length ? fm : undefined,
          links: [],
        })
        total += result.promoted
      } catch {
        failed++
      }
    }
    setPromotingAll(false)
    await load()
    const promoted = datasets.length - skipped - failed
    if (skipped === datasets.length) {
      toast.error('没有已保存映射的 Dataset，请先逐个选择 Entity Type 并点击「保存为 Entity Type」')
    } else if (failed) {
      toast.error(`${failed} 个失败，${promoted} 个成功`)
    } else {
      toast.success(`全部 Promote 完成：${promoted} 个 Dataset，共 ${total} 条记录`)
    }
  }

  const selectedDs = datasets.find(d => d.id === selectedId)

  function handleSelect(ds: Dataset) {
    selectDataset(ds.id)
  }

  const configuredCount = datasets.filter(d => d.entity_type_id != null).length

  return (
    <div className="flex h-full overflow-hidden">
      {/* Left: Dataset list */}
      <div className="w-72 flex-shrink-0 border-r border-slate-800 flex flex-col overflow-hidden">
        <div className="flex items-center justify-between px-4 py-3 border-b border-slate-800 flex-shrink-0">
          <div>
            <p className="text-sm font-semibold text-white">Raw Datasets</p>
            <p className="text-xs text-slate-500 mt-0.5">Select a dataset to promote</p>
          </div>
          <div className="flex items-center gap-2">
            {datasets.length > 1 && (
              <button
                onClick={handlePromoteAll}
                disabled={promotingAll}
                className="text-xs px-2 py-1 rounded bg-indigo-700/60 hover:bg-indigo-600/80 text-indigo-200 disabled:opacity-50 transition-colors"
                title="将所有已配置映射的 Dataset 一起 Promote，确保 DDD 角色推断完备"
              >
                {promotingAll ? '…' : configuredCount > 0 ? `全部 Promote (${configuredCount})` : '全部 Promote'}
              </button>
            )}
            <button
              onClick={load}
              className="text-slate-500 hover:text-slate-300 transition-colors text-sm"
              title="刷新"
            >↻</button>
          </div>
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
