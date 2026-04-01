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
        const { mapping, links } = await datasetsApi.getMapping(ds.id) as { mapping: any, links?: any[] }
        if (!mapping?.entity_type_id) { skipped++; continue }
        const fm = mapping.field_mapping
          ? (typeof mapping.field_mapping === 'string'
              ? JSON.parse(mapping.field_mapping as unknown as string)
              : mapping.field_mapping)
          : undefined
        const savedLinks = (links ?? []).map((l: any) => ({
          from_fk_col: l.from_fk_col,
          to_entity_type_id: l.to_entity_type_id,
          rel_type: l.rel_type,
        }))
        const result = await datasetsApi.promote(ds.id, {
          entity_type_id: mapping.entity_type_id,
          new_entity_type: undefined,
          sync_mode: mapping.sync_mode ?? 'snapshot',
          primary_key_col: mapping.primary_key_col ?? undefined,
          field_mapping: fm && Object.keys(fm).length ? fm : undefined,
          links: savedLinks,
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

  return (
    <div className="flex flex-1 min-h-0 overflow-hidden">
      {/* Left: Dataset list */}
      <div className="w-72 flex-shrink-0 border-r border-slate-800 flex flex-col overflow-hidden">
        <div className="flex items-center justify-between px-3 py-2 border-b border-slate-800 flex-shrink-0">
          <p className="text-sm font-semibold text-white">Raw Datasets</p>
          <div className="flex items-center gap-2">
            <button
              onClick={handlePromoteAll}
              disabled={promotingAll || datasets.length === 0}
              className="px-2.5 py-1 text-xs font-medium rounded-lg bg-indigo-600 hover:bg-indigo-500 text-white disabled:opacity-40 transition-colors"
            >
              {promotingAll ? 'Promoting…' : 'Promote All'}
            </button>
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
