import { useState } from 'react'
import { toast } from 'sonner'
import { boundedContextsApi, projectsApi } from '@/api'
import type { Fold, BcSuggestion } from '@/api'
import { cn } from '@/lib/utils'

const CONFIDENCE_COLOR = (c: number) =>
  c >= 0.7 ? 'text-emerald-400' : c >= 0.4 ? 'text-amber-400' : 'text-slate-500'

const DDD_BADGE: Record<string, string> = {
  aggregate_root: 'AR', value_object: 'VO', domain_event: 'DE', service: 'SV',
}

interface SuggestionState {
  suggestion: BcSuggestion
  accepted: boolean
  name: string
}

interface Props {
  projectId: string
  onApplied?: () => void
}

export default function BcInferencePanel({ projectId, onApplied }: Props) {
  const [folds,      setFolds]      = useState<Fold[]>([])
  const [foldId,     setFoldId]     = useState('')
  const [loading,    setLoading]    = useState(false)
  const [applying,   setApplying]   = useState(false)
  const [items,      setItems]      = useState<SuggestionState[]>([])
  const [ran,        setRan]        = useState(false)

  // Load folds once
  async function ensureFolds() {
    if (folds.length) return
    try {
      const fs = await projectsApi.folds(projectId)
      setFolds(fs)
      if (fs.length) setFoldId(fs[0].id)
    } catch (e) {
      toast.error(String(e))
    }
  }

  async function handleInfer() {
    if (!foldId) { toast.error('请选择 Fold'); return }
    setLoading(true)
    setRan(false)
    try {
      const result = await boundedContextsApi.inferChildBcs(foldId)
      const states: SuggestionState[] = result.suggestions.map(s => ({
        suggestion: s,
        accepted: true,
        name: s.suggested_name,
      }))
      setItems(states)
      setRan(true)
      if (!result.suggestions.length) toast.info('未发现可分组的 BC — 数据连接太少或 ET 不足')
    } catch (e) {
      toast.error(String(e))
    } finally {
      setLoading(false)
    }
  }

  async function handleApply() {
    const accepted = items.filter(i => i.accepted)
    if (!accepted.length) { toast.error('没有接受任何 BC'); return }
    setApplying(true)
    try {
      const payload = accepted.map(i => ({
        ...i.suggestion,
        suggested_name: i.name,
      }))
      await boundedContextsApi.applyBcSuggestions(foldId, payload)
      toast.success(`已应用 ${accepted.length} 个 BC`)
      setRan(false)
      setItems([])
      onApplied?.()
    } catch (e) {
      toast.error(String(e))
    } finally {
      setApplying(false)
    }
  }

  return (
    <div className="border-t border-slate-800 pt-3 mt-3">
      <p className="text-[11px] font-semibold text-slate-400 uppercase tracking-wider px-3 mb-2">
        BC 自动推断
      </p>

      {/* Fold selector + Infer button */}
      <div className="px-3 flex gap-2 items-center mb-3">
        <select
          value={foldId}
          onFocus={ensureFolds}
          onChange={e => { setFoldId(e.target.value); setRan(false); setItems([]) }}
          className="flex-1 min-w-0 bg-slate-950 border border-slate-700 rounded px-2 py-1 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
        >
          {!folds.length && <option value="">— 选择 Fold —</option>}
          {folds.map(f => (
            <option key={f.id} value={f.id}>{f.name}</option>
          ))}
        </select>
        <button
          onClick={handleInfer}
          disabled={loading || !foldId}
          className="flex-shrink-0 px-2.5 py-1 text-xs bg-indigo-700 hover:bg-indigo-600 disabled:opacity-50 text-white rounded transition-colors"
        >
          {loading ? '推断中…' : '↻ 推断'}
        </button>
      </div>

      {/* Results */}
      {ran && items.length === 0 && (
        <p className="px-3 text-xs text-slate-600">该 Fold 中未检测到可分组的 BC</p>
      )}

      {items.length > 0 && (
        <div className="px-3 space-y-2">
          {items.map((item, idx) => (
            <div
              key={idx}
              className={cn(
                'rounded-lg border p-3 space-y-2',
                item.accepted
                  ? 'bg-slate-900 border-indigo-800/50'
                  : 'bg-slate-950 border-slate-800 opacity-50',
              )}
            >
              {/* BC name + accept toggle */}
              <div className="flex items-center gap-2">
                <input
                  type="checkbox"
                  checked={item.accepted}
                  onChange={e => setItems(prev => prev.map((it, i) => i === idx ? { ...it, accepted: e.target.checked } : it))}
                  className="accent-indigo-500 flex-shrink-0"
                />
                <input
                  value={item.name}
                  onChange={e => setItems(prev => prev.map((it, i) => i === idx ? { ...it, name: e.target.value } : it))}
                  disabled={!item.accepted}
                  className="flex-1 bg-slate-950 border border-slate-700 rounded px-2 py-0.5 text-xs text-slate-200 focus:outline-none focus:border-indigo-500 disabled:opacity-50"
                />
                <span className={cn('text-[10px] font-mono flex-shrink-0', CONFIDENCE_COLOR(item.suggestion.confidence))}>
                  {Math.round(item.suggestion.confidence * 100)}%
                </span>
              </div>

              {/* ETs */}
              <div className="flex flex-wrap gap-1">
                {item.suggestion.entity_types.map(et => (
                  <span
                    key={et.id}
                    className={cn(
                      'text-[10px] px-1.5 py-0.5 rounded bg-slate-800 text-slate-400',
                      et.id === item.suggestion.aggregate_root_id && 'bg-indigo-900/50 text-indigo-300 font-semibold',
                    )}
                  >
                    {et.display_name || et.name}
                    {et.ddd_role && et.ddd_role !== 'entity' && (
                      <span className="text-slate-600 ml-1">{DDD_BADGE[et.ddd_role] ?? ''}</span>
                    )}
                    {et.id === item.suggestion.aggregate_root_id && (
                      <span className="ml-1 text-indigo-400">★</span>
                    )}
                  </span>
                ))}
              </div>

              {/* Cross-BC links warning */}
              {item.suggestion.cross_bc_links.length > 0 && (
                <p className="text-[10px] text-amber-500/80">
                  ⚠ {item.suggestion.cross_bc_links.length} 条跨 BC 边（将产生 BC Relationship）
                </p>
              )}
            </div>
          ))}

          {/* Apply button */}
          <button
            onClick={handleApply}
            disabled={applying || items.every(i => !i.accepted)}
            className="w-full mt-2 py-2 text-xs bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white rounded-lg transition-colors"
          >
            {applying ? '应用中…' : `应用 ${items.filter(i => i.accepted).length} 个 BC`}
          </button>
        </div>
      )}
    </div>
  )
}
