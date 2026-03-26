import { useEffect, useRef, useState } from 'react'
import * as d3 from 'd3'
import { boundedContextsApi } from '@/api'
// ── Types ──────────────────────────────────────────────────────────────────

interface BcNode {
  id: string; fold_id: string; fold_name: string
  name: string; color: string; auto_detected: boolean
  et_count: number
}
interface BcEdge {
  id: string; from_bc_id: string; to_bc_id: string
  relationship_type: string; notes?: string
}
interface ContextMapData {
  bounded_contexts: BcNode[]
  relationships: BcEdge[]
  is_fold_fallback?: boolean
}

interface SimNode extends d3.SimulationNodeDatum, BcNode {}
interface SimLink extends d3.SimulationLinkDatum<SimNode> {
  raw: BcEdge
}

// ── Relationship type styles ───────────────────────────────────────────────

const REL_STYLE: Record<string, { label: string; color: string; dash: string }> = {
  shared_kernel:     { label: '共享内核',   color: '#6366f1', dash: '0' },
  customer_supplier: { label: '客户/供应商', color: '#10b981', dash: '6,3' },
  conformist:        { label: '顺从者',     color: '#f59e0b', dash: '4,2,1,2' },
  acl:               { label: '防腐层',     color: '#ec4899', dash: '2,4' },
  open_host:         { label: '开放主机',   color: '#3b82f6', dash: '8,2' },
}

const REL_TYPES = Object.keys(REL_STYLE)

// ── Component ──────────────────────────────────────────────────────────────

interface Props { projectId: string }

export default function ContextMapTab({ projectId }: Props) {
  const svgRef    = useRef<SVGSVGElement>(null)
  const [data,    setData]    = useState<ContextMapData | null>(null)
  const [loading, setLoading] = useState(true)
  const [showAdd, setShowAdd] = useState(false)
  const [addRel,  setAddRel]  = useState({ from: '', to: '', type: 'shared_kernel', notes: '' })
  const [saving,  setSaving]  = useState(false)

  async function load() {
    setLoading(true)
    try {
      const d = await boundedContextsApi.contextMap(projectId) as ContextMapData
      setData(d)
    } catch (e) {
      console.error(e)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => { load() }, [projectId])

  // ── D3 force graph ────────────────────────────────────────────────────────

  useEffect(() => {
    if (!data || !svgRef.current) return
    const svg = d3.select(svgRef.current)
    svg.selectAll('*').remove()

    const W = svgRef.current.clientWidth  || 800
    const H = svgRef.current.clientHeight || 500

    const nodes: SimNode[] = data.bounded_contexts.map(bc => ({ ...bc }))
    // SimLink wraps BcEdge and provides source/target as node IDs for D3
    const simLinks: SimLink[] = data.relationships.map(e => ({
      raw: e,
      source: e.from_bc_id,
      target: e.to_bc_id,
    }))

    if (!nodes.length) return


    // defs: arrowheads per rel type
    const defs = svg.append('defs')
    REL_TYPES.forEach(rt => {
      const s = REL_STYLE[rt]
      defs.append('marker')
        .attr('id', `arrow-${rt}`)
        .attr('viewBox', '0 -5 10 10')
        .attr('refX', 22).attr('refY', 0)
        .attr('markerWidth', 6).attr('markerHeight', 6)
        .attr('orient', 'auto')
        .append('path').attr('d', 'M0,-5L10,0L0,5')
        .attr('fill', s.color)
    })

    const g = svg.append('g')

    // Zoom
    svg.call(d3.zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.2, 3])
      .on('zoom', ev => g.attr('transform', ev.transform))
    )

    // Links
    const link = g.append('g').selectAll<SVGLineElement, SimLink>('line')
      .data(simLinks).join('line')
      .attr('stroke', d => (REL_STYLE[d.raw.relationship_type]?.color ?? '#6b7280'))
      .attr('stroke-width', 2)
      .attr('stroke-dasharray', d => (REL_STYLE[d.raw.relationship_type]?.dash ?? '0'))
      .attr('marker-end', d => `url(#arrow-${d.raw.relationship_type})`)

    // Link labels
    const linkLabel = g.append('g').selectAll<SVGTextElement, SimLink>('text')
      .data(simLinks).join('text')
      .attr('font-size', 9)
      .attr('fill', d => REL_STYLE[d.raw.relationship_type]?.color ?? '#6b7280')
      .attr('text-anchor', 'middle')
      .text(d => REL_STYLE[d.raw.relationship_type]?.label ?? d.raw.relationship_type)

    // Node groups
    const node = g.append('g').selectAll<SVGGElement, SimNode>('g')
      .data(nodes).join('g')
      .attr('cursor', 'grab')
      .call(
        d3.drag<SVGGElement, SimNode>()
          .on('start', (ev, d) => { if (!ev.active) sim.alphaTarget(0.3).restart(); d.fx = d.x; d.fy = d.y })
          .on('drag',  (ev, d) => { d.fx = ev.x; d.fy = ev.y })
          .on('end',   (ev, d) => { if (!ev.active) sim.alphaTarget(0); d.fx = null; d.fy = null })
      )

    // Node rect
    node.append('rect')
      .attr('width', 120).attr('height', 52)
      .attr('x', -60).attr('y', -26)
      .attr('rx', 8)
      .attr('fill', d => d.color + '22')
      .attr('stroke', d => d.color)
      .attr('stroke-width', 1.5)

    // BC name
    node.append('text')
      .attr('dy', -8)
      .attr('text-anchor', 'middle')
      .attr('fill', '#e2e8f0')
      .attr('font-size', 11)
      .attr('font-weight', 600)
      .text(d => d.name.length > 14 ? d.name.slice(0, 13) + '…' : d.name)

    // Fold name
    node.append('text')
      .attr('dy', 6)
      .attr('text-anchor', 'middle')
      .attr('fill', '#64748b')
      .attr('font-size', 9)
      .text(d => d.fold_name.length > 16 ? d.fold_name.slice(0, 15) + '…' : d.fold_name)

    // ET count badge
    node.append('text')
      .attr('dy', 20)
      .attr('text-anchor', 'middle')
      .attr('fill', '#475569')
      .attr('font-size', 9)
      .text(d => `${d.et_count} ET${d.auto_detected ? ' · auto' : ''}`)

    // Simulation
    const sim = d3.forceSimulation<SimNode>(nodes)
      .force('link', d3.forceLink<SimNode, SimLink>(simLinks)
        .id(d => d.id).distance(200).strength(0.4))
      .force('charge', d3.forceManyBody().strength(-400))
      .force('center', d3.forceCenter(W / 2, H / 2))
      .force('collision', d3.forceCollide(80))
      .on('tick', () => {
        link
          .attr('x1', d => (d.source as SimNode).x ?? 0)
          .attr('y1', d => (d.source as SimNode).y ?? 0)
          .attr('x2', d => (d.target as SimNode).x ?? 0)
          .attr('y2', d => (d.target as SimNode).y ?? 0)

        linkLabel
          .attr('x', d => (((d.source as SimNode).x ?? 0) + ((d.target as SimNode).x ?? 0)) / 2)
          .attr('y', d => (((d.source as SimNode).y ?? 0) + ((d.target as SimNode).y ?? 0)) / 2 - 6)

        node.attr('transform', d => `translate(${d.x ?? 0},${d.y ?? 0})`)
      })

    return () => { sim.stop() }
  }, [data])

  async function handleAddRel() {
    if (!addRel.from || !addRel.to || addRel.from === addRel.to) return
    setSaving(true)
    try {
      await boundedContextsApi.createRelationship({
        from_bc_id: addRel.from,
        to_bc_id: addRel.to,
        relationship_type: addRel.type,
        notes: addRel.notes || undefined,
      })
      setShowAdd(false)
      setAddRel({ from: '', to: '', type: 'shared_kernel', notes: '' })
      load()
    } catch (e) {
      console.error(e)
    } finally {
      setSaving(false)
    }
  }

  if (loading) return <div className="p-8 text-slate-500 text-sm">加载中…</div>

  const bcs = data?.bounded_contexts ?? []

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Toolbar */}
      <div className="flex items-center gap-3 px-4 py-2 border-b border-slate-800 flex-shrink-0">
        <span className="text-xs text-slate-400 font-semibold">Context Map</span>
        <span className="text-xs text-slate-600">{bcs.length} BC · {data?.relationships.length ?? 0} 关系</span>
        {data?.is_fold_fallback && (
          <span className="text-xs text-amber-500/80 border border-amber-800/40 rounded px-1.5 py-0.5">
            Fold 视图 — 运行 BC 推断后升级
          </span>
        )}
        <button
          onClick={() => setShowAdd(v => !v)}
          className="ml-auto text-xs px-3 py-1.5 bg-slate-800 border border-slate-700 hover:border-indigo-600 text-slate-300 rounded-lg transition-colors"
        >
          + 添加 BC 关系
        </button>
        <button onClick={load} className="text-xs px-3 py-1.5 border border-slate-700 text-slate-500 hover:text-slate-300 rounded-lg transition-colors">
          ↻
        </button>
      </div>

      {/* Add relationship form */}
      {showAdd && (
        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-slate-800 bg-slate-900/60 flex-shrink-0 flex-wrap">
          <select
            value={addRel.from}
            onChange={e => setAddRel(v => ({ ...v, from: e.target.value }))}
            className="bg-slate-950 border border-slate-700 rounded px-2 py-1 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
          >
            <option value="">— 源 BC —</option>
            {bcs.map(bc => <option key={bc.id} value={bc.id}>{bc.name}</option>)}
          </select>
          <span className="text-slate-600 text-xs">→</span>
          <select
            value={addRel.to}
            onChange={e => setAddRel(v => ({ ...v, to: e.target.value }))}
            className="bg-slate-950 border border-slate-700 rounded px-2 py-1 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
          >
            <option value="">— 目标 BC —</option>
            {bcs.filter(bc => bc.id !== addRel.from).map(bc => <option key={bc.id} value={bc.id}>{bc.name}</option>)}
          </select>
          <select
            value={addRel.type}
            onChange={e => setAddRel(v => ({ ...v, type: e.target.value }))}
            className="bg-slate-950 border border-slate-700 rounded px-2 py-1 text-xs text-slate-200 focus:outline-none focus:border-indigo-500"
          >
            {REL_TYPES.map(rt => <option key={rt} value={rt}>{REL_STYLE[rt].label}</option>)}
          </select>
          <input
            value={addRel.notes}
            onChange={e => setAddRel(v => ({ ...v, notes: e.target.value }))}
            placeholder="备注（可选）"
            className="bg-slate-950 border border-slate-700 rounded px-2 py-1 text-xs text-slate-200 focus:outline-none focus:border-indigo-500 w-36"
          />
          <button
            onClick={handleAddRel}
            disabled={saving || !addRel.from || !addRel.to}
            className="px-3 py-1 text-xs bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white rounded transition-colors"
          >
            {saving ? '…' : '确定'}
          </button>
          <button onClick={() => setShowAdd(false)} className="text-xs text-slate-600 hover:text-slate-400">取消</button>
        </div>
      )}

      {/* Legend */}
      <div className="flex items-center gap-4 px-4 py-1.5 border-b border-slate-800 flex-shrink-0 flex-wrap">
        {REL_TYPES.map(rt => (
          <div key={rt} className="flex items-center gap-1.5">
            <svg width="24" height="4">
              <line x1="0" y1="2" x2="24" y2="2"
                stroke={REL_STYLE[rt].color}
                strokeWidth="2"
                strokeDasharray={REL_STYLE[rt].dash}
              />
            </svg>
            <span className="text-[10px] text-slate-500">{REL_STYLE[rt].label}</span>
          </div>
        ))}
      </div>

      {/* Canvas */}
      <div className="flex-1 overflow-hidden relative">
        {bcs.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-center">
            <p className="text-slate-500 text-sm mb-1">暂无 Fold / Bounded Context</p>
            <p className="text-slate-600 text-xs">先在「数据接入」创建数据源，或在「模型」运行 BC 推断</p>
          </div>
        ) : (
          <svg ref={svgRef} className="w-full h-full" />
        )}
      </div>
    </div>
  )
}
