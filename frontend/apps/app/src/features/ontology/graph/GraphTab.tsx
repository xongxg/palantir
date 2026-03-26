import { useEffect, useRef, useState, useCallback } from 'react'
import * as d3 from 'd3'
import { toast } from 'sonner'
import { graphApi, entityTypesApi } from '@/api'
import { buildBCsFromFolds, classifyDDDRoles } from './bcInference'
import type { BC, GNode, GEdge, GFold, EtSummary } from './bcInference'

// ── Types matching the graph API response ──────────────────────────────────
// SimNode extends SimulationNodeDatum so D3 can add x/y/fx/fy
interface SimNode extends d3.SimulationNodeDatum {
  id: string; label: string; et_id: string; et_name: string
  color: string; fold_id?: string; is_default: boolean
  props: Record<string, unknown>
}
type ApiNode = SimNode
interface ApiEdge  { source: string; target: string; label?: string }
interface GraphData {
  nodes: ApiNode[]; edges: ApiEdge[]
  folds: GFold[]; et_summary: EtSummary[]
}

// DDD role → ring/indicator color (overlaid on top of ET type color)
const DDD_RING: Record<string, string> = {
  'aggregate-root': '#f85149',   // red ring = AR
  'value-object':   '#3fb950',   // green dashed = VO
  'entity':         'none',
}
const ROLE_LABEL: Record<string, string> = {
  'aggregate-root': '◆ Aggregate Root',
  'entity':         '▸ Entity',
  'value-object':   '○ Value Object',
}

// Per-ET-type distinct color palette (matches backend)
const ET_PALETTE = [
  '#6366f1','#8b5cf6','#ec4899','#14b8a6','#f59e0b',
  '#10b981','#3b82f6','#f97316','#ef4444','#84cc16',
  '#06b6d4','#d946ef','#a855f7','#22c55e','#0ea5e9',
]
const DEFAULT_ET_COLOR = '#6366f1'
function hashEtColor(id: string): string {
  let h = 0
  for (let i = 0; i < id.length; i++) h = Math.imul(31, h) + id.charCodeAt(i)
  return ET_PALETTE[Math.abs(h) % ET_PALETTE.length]
}

function escHtml(s: string) {
  return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;')
}

// ── Controls bar ──────────────────────────────────────────────────────────
interface ControlsProps {
  frozen: boolean; labels: boolean; hulls: boolean
  onFreeze: () => void; onLabels: () => void; onHulls: () => void; onFullscreen: () => void
  onAutoLink: () => void; linking: boolean
  loading: boolean
}
function Controls({ frozen, labels, hulls, onFreeze, onLabels, onHulls, onFullscreen, onAutoLink, linking, loading }: ControlsProps) {
  const btn = (active: boolean, onClick: () => void, text: string) => (
    <button
      onClick={onClick}
      className={`px-3 py-1.5 text-xs rounded-lg border transition-colors ${
        active
          ? 'bg-indigo-600 border-indigo-500 text-white'
          : 'bg-slate-800 border-slate-700 text-slate-400 hover:text-slate-200'
      }`}
    >{text}</button>
  )
  return (
    <div className="flex items-center gap-2 px-4 py-2 border-b border-slate-800 flex-shrink-0">
      {loading && <span className="text-xs text-slate-500 mr-2">加载中…</span>}
      {btn(frozen, onFreeze, frozen ? '▶ 解冻' : '❄ 冻结')}
      {btn(labels, onLabels, labels ? '标签：开' : '标签：关')}
      {btn(hulls,  onHulls,  hulls  ? 'BC 边界：开' : 'BC 边界：关')}
      <button
        onClick={onAutoLink}
        disabled={linking}
        className="px-3 py-1.5 text-xs rounded-lg border bg-slate-800 border-slate-700 text-slate-400 hover:text-emerald-400 hover:border-emerald-700 disabled:opacity-50 transition-colors"
        title="扫描所有 *_id 列自动建立实体关联"
      >{linking ? '解析中…' : '⚡ 自动关联'}</button>
      <button
        onClick={onFullscreen}
        className="ml-auto px-3 py-1.5 text-xs rounded-lg border bg-slate-800 border-slate-700 text-slate-400 hover:text-slate-200 transition-colors"
      >⛶ 全屏</button>
    </div>
  )
}

// ── Main component ─────────────────────────────────────────────────────────
interface Props { projectId?: string }

export default function GraphTab({ projectId }: Props) {
  const containerRef = useRef<HTMLDivElement>(null)
  const svgRef       = useRef<SVGSVGElement>(null)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const simRef       = useRef<d3.Simulation<any, any> | null>(null)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const hullLayerRef    = useRef<d3.Selection<any, any, any, any> | null>(null)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const etGroupLayerRef = useRef<d3.Selection<any, any, any, any> | null>(null)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const labelSelRef     = useRef<d3.Selection<any, any, any, any> | null>(null)

  const [loading, setLoading]   = useState(true)
  const [frozen,  setFrozen]    = useState(false)
  const [labels,  setLabels]    = useState(true)
  const [hulls,   setHulls]     = useState(true)
  const [linking, setLinking]   = useState(false)
  const frozenRef = useRef(false)

  // ── Draw graph ───────────────────────────────────────────────────────────
  useEffect(() => {
    let cancelled = false
    setLoading(true)

    graphApi.get(projectId).then(data => {
      if (cancelled) return
      setLoading(false)
      if (svgRef.current) drawGraph(data as unknown as GraphData, svgRef.current)
    }).catch(() => setLoading(false))

    return () => { cancelled = true; simRef.current?.stop() }
  }, [projectId])

  // ── Sync control toggles back into D3 state ──────────────────────────────
  function handleFreeze() {
    const next = !frozen
    setFrozen(next)
    frozenRef.current = next
    const sim = simRef.current
    if (sim) {
      if (next) { sim.stop() }
      else {
        sim.alphaTarget(0.1).restart()
        setTimeout(() => simRef.current?.alphaTarget(0), 2000)
      }
    }
  }

  function handleLabels() {
    const next = !labels
    setLabels(next)
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    ;(labelSelRef.current as any)?.style('opacity', next ? null : '0')
  }

  function handleHulls() {
    const next = !hulls
    setHulls(next)
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    ;(hullLayerRef.current as any)?.style('display', next ? null : 'none')
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    ;(etGroupLayerRef.current as any)?.style('display', next ? null : 'none')
  }

  const handleAutoLink = useCallback(async () => {
    setLinking(true)
    try {
      const res = await graphApi.autoLink()
      toast.success(`已自动建立 ${res.created} 条关联（${res.skipped} 条未匹配）`)
      // Reload graph after linking
      if (res.created > 0 && svgRef.current) {
        graphApi.get(projectId).then(data => {
          if (svgRef.current) drawGraph(data as unknown as GraphData, svgRef.current)
        }).catch(() => {})
      }
    } catch (e) {
      toast.error(String(e))
    } finally {
      setLinking(false)
    }
  }, [projectId])

  function handleFullscreen() {
    const el = containerRef.current
    if (!el) return
    if (!document.fullscreenElement) el.requestFullscreen?.()
    else document.exitFullscreen?.()
  }

  // ── Core D3 rendering ────────────────────────────────────────────────────
  function drawGraph(data: GraphData, svgEl: SVGSVGElement) {
    const { nodes, edges, folds, et_summary } = data
    simRef.current?.stop()

    const svg = d3.select(svgEl)
    svg.selectAll('*').remove()
    d3.select('#graph-tooltip-rf').remove()
    d3.select('#graph-role-menu').remove()
    d3.select('body').on('click.rolemenu', null)
    d3.select('body').on('keydown.rolemenu', null)
    hullLayerRef.current    = null
    etGroupLayerRef.current = null
    labelSelRef.current     = null

    const rect = svgEl.getBoundingClientRect()
    const W = (rect.width  > 10 ? rect.width  : window.innerWidth  - 32) || 900
    const H = (rect.height > 10 ? rect.height : window.innerHeight - 160) || 580

    svgEl.setAttribute('width',  String(W))
    svgEl.setAttribute('height', String(H))

    if (!nodes.length) {
      svg.append('text')
        .attr('x', W/2).attr('y', H/2)
        .attr('text-anchor','middle').attr('fill','#475569').attr('font-size', 14)
        .text('暂无对象 — 先在 Import tab 配置映射并 Promote')
      return
    }

    // nodes already implement SimulationNodeDatum via SimNode interface
    const simNodes = nodes
    const simEdges = edges.map(e => ({ ...e })) as (ApiEdge & { source: ApiNode | string; target: ApiNode | string })[]

    // BC inference + DDD roles
    const gNodes: GNode[] = simNodes.map(n => ({ id: n.id, et_id: n.et_id, fold_id: n.fold_id, is_default: n.is_default }))
    const gEdges: GEdge[] = edges.map(e => ({ source: e.source, target: e.target }))
    const validEdgesRaw   = gEdges.filter(e => {
      const s = typeof e.source === 'object' ? (e.source as GNode).id : e.source
      const t = typeof e.target === 'object' ? (e.target as GNode).id : e.target
      return s && t && s !== t
    })
    const { bcs, nodeBC } = buildBCsFromFolds(gNodes, folds as GFold[], et_summary, validEdgesRaw)
    const etRoles          = classifyDDDRoles(et_summary, validEdgesRaw, gNodes)

    // node id → bc.id
    const nodeBCId: Record<string, string> = {}
    bcs.forEach(bc => bc.ids.forEach(id => { nodeBCId[id] = bc.id }))

    // Node radius
    // Build ET type → color map: use DB color if distinct, else deterministic hash
    const etColorMap: Record<string, string> = {}
    et_summary.forEach(e => {
      etColorMap[e.id] = (e.color && e.color !== DEFAULT_ET_COLOR)
        ? e.color
        : hashEtColor(e.id)
    })

    const R = nodes.length > 80 ? 9 : nodes.length > 30 ? 13 : 17
    const nodeR = (d: ApiNode) => {
      if (d.is_default) return 2
      const role = etRoles[d.et_id] || 'entity'
      return role === 'aggregate-root' ? Math.round(R * 1.45) : role === 'entity' ? R : Math.round(R * 0.67)
    }
    // Primary color = ET type's own color; DDD role → ring/style overlay
    const etColor = (d: ApiNode) => etColorMap[d.et_id] || (d.color && d.color !== DEFAULT_ET_COLOR ? d.color : hashEtColor(d.et_id))

    // Initial positions: BC centroid layout
    const bcAngle   = (2 * Math.PI) / Math.max(bcs.length, 1)
    const bcSpread  = Math.min(W, H) * 0.33
    const bcCentroids: Record<string, { x: number; y: number }> = {}
    bcs.forEach((bc, i) => {
      bcCentroids[bc.id] = {
        x: W/2 + bcSpread * Math.cos(i * bcAngle - Math.PI/2),
        y: H/2 + bcSpread * Math.sin(i * bcAngle - Math.PI/2),
      }
    })
    const etCentroids: Record<string, { x: number; y: number }> = {}
    bcs.forEach(bc => {
      const bcC   = bcCentroids[bc.id] || { x: W/2, y: H/2 }
      const etIds = [...new Set(simNodes.filter(n => bc.ids.includes(n.id)).map(n => n.et_id))]
      etIds.forEach((etId, j) => {
        const a = (2 * Math.PI / Math.max(etIds.length, 1)) * j
        const r = bcSpread * 0.42
        etCentroids[etId] = { x: bcC.x + r * Math.cos(a), y: bcC.y + r * Math.sin(a) }
      })
    })
    // Fallback: when no BCs, position each ET type in a circle so same-type nodes cluster
    if (!bcs.length) {
      const uniqueETIds = [...new Set(simNodes.filter(n => !n.is_default).map(n => n.et_id))]
      uniqueETIds.forEach((etId, i) => {
        const a = (2 * Math.PI * i) / uniqueETIds.length - Math.PI / 2
        etCentroids[etId] = {
          x: W/2 + bcSpread * Math.cos(a),
          y: H/2 + bcSpread * Math.sin(a),
        }
      })
    }
    simNodes.filter(n => !n.is_default).forEach(n => {
      const c = etCentroids[n.et_id] || { x: W/2, y: H/2 }
      n.x = c.x + (Math.random() - 0.5) * R * 4
      n.y = c.y + (Math.random() - 0.5) * R * 4
    })
    const outerR = Math.min(W, H) * 0.48
    const defNodes = simNodes.filter(n => n.is_default)
    defNodes.forEach((n, i) => {
      const a = (2 * Math.PI * i) / Math.max(defNodes.length, 1)
      n.x = W/2 + outerR * Math.cos(a)
      n.y = H/2 + outerR * Math.sin(a)
    })

    // SVG scaffold
    const zoom = d3.zoom<SVGSVGElement, unknown>().scaleExtent([0.04, 8]).on('zoom', e => g.attr('transform', e.transform.toString()))
    svg.call(zoom)
    const g = svg.append('g')

    // Arrow markers
    const defs = svg.append('defs')
    ;([{ id:'arr-intra-rf', color:'#388bfd' }, { id:'arr-cross-rf', color:'#f97316' }]).forEach(({ id, color }) => {
      defs.append('marker').attr('id', id)
        .attr('viewBox','0 -4 8 8').attr('refX', 8).attr('refY', 0)
        .attr('markerWidth', 6).attr('markerHeight', 6).attr('orient','auto')
        .append('path').attr('d','M0,-4L8,0L0,4').attr('fill', color).attr('opacity', 0.9)
    })

    // ET-type grouping layer (bottom — subtle dashed outlines, one per ET type)
    interface EtGroup { etId: string; label: string; color: string; nodeIds: string[] }
    const etGroupData: EtGroup[] = [...new Set(
      simNodes.filter(n => !n.is_default).map(n => n.et_id)
    )].map(etId => ({
      etId,
      label: et_summary.find(e => e.id === etId)?.label ?? etId,
      color: etColorMap[etId] || '#388bfd',
      nodeIds: simNodes.filter(n => n.et_id === etId && !n.is_default).map(n => n.id),
    }))
    const etGroupLayer = g.append('g')
    etGroupLayerRef.current = etGroupLayer
    const etGroupPaths = etGroupLayer.selectAll<SVGPathElement, EtGroup>('path').data(etGroupData).join('path')
      .attr('fill', d => d.color + '12')
      .attr('stroke', d => d.color)
      .attr('stroke-opacity', 0.25)
      .attr('stroke-width', 1)
      .attr('stroke-dasharray', '5,4')
      .attr('stroke-linejoin', 'round')
      .attr('pointer-events', 'none')
    const etGroupLabels = etGroupLayer.selectAll<SVGTextElement, EtGroup>('text').data(etGroupData).join('text')
      .attr('font-size', 9).attr('font-weight', 600)
      .attr('fill', d => d.color).attr('opacity', 0.4)
      .attr('text-anchor', 'middle').attr('pointer-events', 'none')
      .text(d => d.label)

    // Hull layer (BC boundaries — on top of ET groups)
    const hullLayer = g.append('g')
    hullLayerRef.current = hullLayer
    const hullPaths = hullLayer.selectAll<SVGPathElement, BC>('path').data(bcs).join('path')
      .attr('fill',         bc => bc.color)
      .attr('fill-opacity', bc => bc.isFold ? 0.04 : 0.12)
      .attr('stroke',       bc => bc.color)
      .attr('stroke-opacity', bc => bc.isFold ? 0.3 : 0.7)
      .attr('stroke-width', bc => bc.isFold ? 1.5 : 2)
      .attr('stroke-dasharray', bc => bc.isFold ? '8,4' : null)
      .attr('stroke-linejoin','round')
    const hullLabels = hullLayer.selectAll<SVGTextElement, BC>('text').data(bcs).join('text')
      .attr('fill',        bc => bc.color)
      .attr('font-size',   bc => bc.isFold ? 12 : 10)
      .attr('font-weight', 700).attr('letter-spacing', '0.5px')
      .attr('opacity',     bc => bc.isFold ? 0.5 : 0.85)
      .attr('pointer-events','none')
      .text(bc => bc.isFold ? `⬡ ${bc.name}` : `◈ ${bc.name}`)

    // Edge intra/cross helper
    function edgeIsIntra(d: typeof simEdges[0]) {
      const s = typeof d.source === 'object' ? (d.source as ApiNode).id : d.source
      const t = typeof d.target === 'object' ? (d.target as ApiNode).id : d.target
      return nodeBCId[s] && nodeBCId[t] && nodeBCId[s] === nodeBCId[t]
    }

    // Edge layer (only valid edges)
    const validEdgeSels = simEdges.filter(e => {
      const s = typeof e.source === 'string' ? e.source : (e.source as ApiNode).id
      const t = typeof e.target === 'string' ? e.target : (e.target as ApiNode).id
      return s && t && s !== t
    })
    const edgeSel = g.append('g').selectAll<SVGLineElement, typeof simEdges[0]>('line').data(validEdgeSels).join('line')
      .attr('stroke',         d => edgeIsIntra(d) ? '#388bfd' : '#f97316')
      .attr('stroke-opacity', d => edgeIsIntra(d) ? 0.45 : 0.6)
      .attr('stroke-width',   d => edgeIsIntra(d) ? 1 : 1.5)
      .attr('stroke-dasharray', d => edgeIsIntra(d) ? null : '6,3')
      .attr('marker-end', d => `url(#${edgeIsIntra(d) ? 'arr-intra-rf' : 'arr-cross-rf'})`)
    edgeSel.append('title').text(d => d.label || 'HAS')

    // BC cross-link arcs
    const bcCenters: Record<string, { x: number; y: number }> = {}
    const bcPairEdges: Record<string, { from: string; to: string; count: number; _offset?: number }> = {}
    validEdgeSels.forEach(e => {
      const s = typeof e.source === 'object' ? (e.source as ApiNode).id : e.source
      const t = typeof e.target === 'object' ? (e.target as ApiNode).id : e.target
      const sbcId = nodeBCId[s], tbcId = nodeBCId[t]
      if (sbcId && tbcId && sbcId !== tbcId) {
        const key = [sbcId, tbcId].sort().join('|')
        if (!bcPairEdges[key]) bcPairEdges[key] = { from: sbcId, to: tbcId, count: 0 }
        bcPairEdges[key].count++
      }
    })
    const bcLinks = Object.values(bcPairEdges)
    const bcPairGroups: Record<string, typeof bcLinks> = {}
    bcLinks.forEach(cl => { const key = [cl.from, cl.to].sort().join('|'); (bcPairGroups[key] ??= []).push(cl) })
    Object.values(bcPairGroups).forEach(grp => {
      const n = grp.length
      grp.forEach((cl, i) => { cl._offset = (i - (n-1)/2) * 30 })
    })
    const bcLinkLayer = g.append('g')
    const bcLinkSel = bcLinkLayer.selectAll<SVGPathElement, typeof bcLinks[0]>('path').data(bcLinks).join('path')
      .attr('fill','none').attr('stroke','#f0883e').attr('stroke-width', 2)
      .attr('stroke-dasharray','6,4').attr('stroke-opacity', 0.55)
      .attr('pointer-events','none')
    const bcLinkLabels = bcLinkLayer.selectAll<SVGTextElement, typeof bcLinks[0]>('text').data(bcLinks).join('text')
      .attr('font-size', 10).attr('fill','#f0883e').attr('font-weight', 700)
      .attr('paint-order','stroke').attr('stroke','#0d1117').attr('stroke-width', 3).attr('stroke-opacity', 0.9)
      .attr('text-anchor','middle').attr('pointer-events','none')
      .text(cl => `${cl.count}`)

    // Node layer
    const nodeSel = g.append('g').selectAll<SVGGElement, ApiNode>('g').data(simNodes).join('g')
      .style('cursor','pointer')
      .call(
        d3.drag<SVGGElement, ApiNode>()
          .on('start', (ev, d) => {
            if (!ev.active) simRef.current?.alphaTarget(0.3).restart()
            d.fx = d.x; d.fy = d.y
          })
          .on('drag', (ev, d) => { d.fx = ev.x; d.fy = ev.y })
          .on('end', (ev, d) => {
            if (!ev.active) simRef.current?.alphaTarget(0)
            if (!frozenRef.current) { d.fx = null; d.fy = null }
          })
      )

    nodeSel.each(function(d) {
      const sel = d3.select(this)
      const r   = nodeR(d)
      if (d.is_default) {
        sel.append('circle').attr('r', r).attr('fill','#1e293b').attr('stroke','#475569').attr('stroke-width', 0.5).attr('opacity', 0.4)
        return
      }
      const role    = etRoles[d.et_id] || 'entity'
      const color   = etColor(d)           // ET type color
      const ring    = DDD_RING[role]       // DDD role ring color
      const bc      = nodeBC[d.id] as BC | undefined
      const bcColor = bc?.color || '#475569'

      if (role === 'aggregate-root') {
        // ── Aggregate Root: outer glow halo + double ring ─────────────────────
        // Outermost soft glow (wide, semi-transparent)
        sel.append('circle')
          .attr('r', r + 9).attr('fill', ring + '18').attr('stroke', ring)
          .attr('stroke-width', 0.5).attr('stroke-opacity', 0.35).attr('pointer-events','none')
        // BC ring inside the glow
        sel.append('circle').attr('class','node-bc-ring')
          .attr('r', r + 5).attr('fill','none')
          .attr('stroke', bcColor).attr('stroke-width', 1.5).attr('stroke-opacity', 0.55).attr('pointer-events','none')
        // DDD ring: bright solid red
        sel.append('circle').attr('class','node-ddd-ring')
          .attr('r', r + 2).attr('fill','none')
          .attr('stroke', ring).attr('stroke-width', 2.5).attr('stroke-opacity', 1).attr('pointer-events','none')
        // Hover ring
        sel.append('circle').attr('class','node-hover-ring')
          .attr('r', r + 12).attr('fill','none')
          .attr('stroke', color).attr('stroke-width', 2).attr('stroke-opacity', 0).attr('pointer-events','none')
        // Main circle — richer fill for AR
        sel.append('circle').attr('class','node-main')
          .attr('r', r).attr('fill', color + '48').attr('stroke', color).attr('stroke-width', 2.5)
        // ◆ badge at top
        sel.append('text')
          .attr('dy', -r + 3).attr('font-size', Math.max(8, r * 0.55)).attr('text-anchor','middle')
          .attr('fill', ring).attr('font-weight', 700).attr('pointer-events','none')
          .attr('paint-order','stroke').attr('stroke','#0d1117').attr('stroke-width', 2).attr('stroke-opacity', 0.9)
          .text('◆')
      } else {
        // ── Entity / Value Object ─────────────────────────────────────────────
        sel.append('circle').attr('class','node-bc-ring')
          .attr('r', r + 4).attr('fill','none')
          .attr('stroke', bcColor).attr('stroke-width', 1.5).attr('stroke-opacity', 0.5).attr('pointer-events','none')
        if (role === 'value-object') {
          sel.append('circle').attr('class','node-ddd-ring')
            .attr('r', r + 1).attr('fill','none')
            .attr('stroke', ring).attr('stroke-width', 1.5).attr('stroke-dasharray','3,3').attr('stroke-opacity', 0.8).attr('pointer-events','none')
        }
        sel.append('circle').attr('class','node-hover-ring')
          .attr('r', r + 10).attr('fill','none')
          .attr('stroke', color).attr('stroke-width', 2).attr('stroke-opacity', 0).attr('pointer-events','none')
        sel.append('circle').attr('class','node-main')
          .attr('r', r).attr('fill', color + '30').attr('stroke', color).attr('stroke-width', 1.5)
      }

      const lbl = (d.label || '').length > 14 ? d.label.slice(0, 13) + '…' : (d.label || '')
      sel.append('text').attr('class','graph-node-label')
        .attr('dy', r + 11).attr('font-size', 9)
        .attr('fill','#c9d1d9').attr('text-anchor','middle')
        .attr('paint-order','stroke').attr('stroke','#0d1117').attr('stroke-width', 3).attr('stroke-opacity', 0.85)
        .text(lbl)
      // Sub-label: show "AR" badge for aggregate-root, otherwise ET type name
      const subLabel = role === 'aggregate-root' ? 'AR' : (d.et_name || '')
      const subColor  = role === 'aggregate-root' ? DDD_RING['aggregate-root'] : color
      sel.append('text').attr('class','graph-node-label')
        .attr('dy', r + 22).attr('font-size', role === 'aggregate-root' ? 9 : 8)
        .attr('font-weight', role === 'aggregate-root' ? 700 : 400)
        .attr('fill', subColor).attr('opacity', role === 'aggregate-root' ? 0.95 : 0.6)
        .attr('text-anchor','middle')
        .attr('paint-order','stroke').attr('stroke','#0d1117').attr('stroke-width', 2).attr('stroke-opacity', 0.85)
        .text(subLabel)
    })

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    labelSelRef.current = nodeSel.selectAll('text.graph-node-label') as any

    // Staggered entrance animation
    nodeSel.filter(d => !d.is_default)
      .style('opacity', 0)
      .transition().duration(400).delay((_, i) => Math.min(i * 8, 600))
      .ease(d3.easeBackOut.overshoot(1.2))
      .style('opacity', 1)

    // Tooltip
    const tip = d3.select('body').append('div').attr('id','graph-tooltip-rf')
      .style('position','fixed').style('pointer-events','none').style('opacity','0')
      .style('background','#161b22').style('border','1px solid #30363d')
      .style('border-radius','8px').style('padding','10px 14px')
      .style('font-size','12px').style('color','#e6edf3').style('z-index','9999')
      .style('max-width','260px').style('line-height','1.6')
      .style('transition','opacity .15s')

    // Right-click context menu for quick DDD role assignment
    d3.select('#graph-role-menu').remove()
    const roleMenu = d3.select('body').append('div').attr('id','graph-role-menu')
      .style('position','fixed').style('display','none').style('z-index','10000')
      .style('background','#1c2128').style('border','1px solid #30363d')
      .style('border-radius','8px').style('padding','4px').style('min-width','160px')
      .style('box-shadow','0 8px 24px rgba(0,0,0,0.5)')

    const ROLE_MENU_ITEMS = [
      { role: 'aggregate_root', label: '◆ Aggregate Root', color: '#f85149' },
      { role: 'entity',         label: '▸ Entity',          color: '#388bfd' },
      { role: 'value_object',   label: '○ Value Object',    color: '#3fb950' },
    ]

    function hideRoleMenu() { roleMenu.style('display','none') }

    // Close menu on outside click
    d3.select('body').on('click.rolemenu', hideRoleMenu)
    d3.select('body').on('keydown.rolemenu', (ev: KeyboardEvent) => { if (ev.key === 'Escape') hideRoleMenu() })

    nodeSel
      .on('mouseenter', (ev, d) => {
        if (d.is_default) return
        const gEl = d3.select<SVGGElement, ApiNode>(ev.currentTarget)
        gEl.select('.node-main').transition().duration(150).ease(d3.easeBackOut.overshoot(1.5)).attr('transform','scale(1.18)')
        gEl.select('.node-hover-ring').transition().duration(200).attr('stroke-opacity', 0.45).attr('r', nodeR(d) + 10)

        const role    = etRoles[d.et_id] || 'entity'
        const col     = etColor(d)
        const scalarEntries = Object.entries(d.props || {}).filter(([k]) => k !== 'id' && !k.endsWith('_id') && !k.endsWith('Id'))
        const fieldSection  = scalarEntries.map(([k, v]) =>
          `<div style="margin-bottom:1px"><span style="color:#6e7681">${escHtml(k)}:</span> <span style="color:#c9d1d9">${escHtml(String(v ?? ''))}</span></div>`
        ).join('')

        const roleShort = role === 'aggregate-root' ? 'AR' : role === 'value-object' ? 'VO' : 'Entity'
        const roleColor = role === 'aggregate-root' ? '#f85149' : role === 'value-object' ? '#3fb950' : '#6e7681'
        const rolePill  = `<span style="display:inline-block;font-size:10px;font-weight:700;color:${roleColor};` +
          `background:${roleColor}18;border:1px solid ${roleColor}44;border-radius:4px;padding:0 5px;line-height:16px">${roleShort}</span>`
        tip.html(
          `<div style="font-weight:600;color:${col};font-size:13px;margin-bottom:3px">${escHtml(d.label)}</div>` +
          `<div style="display:flex;align-items:center;gap:6px;margin-bottom:${fieldSection?'6px':'2px'}">` +
            `<span style="color:#8b949e;font-size:11px">${escHtml(d.et_name)}</span>${rolePill}` +
          `</div>` +
          (fieldSection ? `<div style="font-size:11px;line-height:1.7;border-top:1px solid #30363d;padding-top:6px">${fieldSection}</div>` : '') +
          `<div style="font-size:10px;color:#3d444d;margin-top:${fieldSection?'6px':'4px'}">右键点击可更改角色</div>`
        ).style('opacity','1')
      })
      .on('mousemove', ev => tip.style('left', (ev.clientX + 14) + 'px').style('top', (ev.clientY - 10) + 'px'))
      .on('mouseleave', (ev, d) => {
        tip.style('opacity','0')
        if (d.is_default) return
        const gEl = d3.select<SVGGElement, ApiNode>(ev.currentTarget)
        gEl.select('.node-main').transition().duration(200).ease(d3.easeCubicOut).attr('transform','scale(1)')
        gEl.select('.node-hover-ring').transition().duration(250).attr('stroke-opacity', 0).attr('r', nodeR(d) + 8)
      })
      .on('contextmenu', function(ev, d) {
        ev.preventDefault()
        ev.stopPropagation()
        if (d.is_default) return
        tip.style('opacity','0')

        const currentRole = etRoles[d.et_id] || 'entity'
        // Build menu items
        roleMenu.html('')
        // Header
        roleMenu.append('div')
          .style('font-size','10px').style('color','#6e7681').style('padding','4px 10px 6px')
          .style('border-bottom','1px solid #30363d').style('margin-bottom','4px')
          .text(`${d.et_name} — DDD 角色`)

        ROLE_MENU_ITEMS.forEach(item => {
          const isActive = item.role === currentRole || item.role === currentRole.replace('-','_')
          roleMenu.append('div')
            .style('padding','6px 10px').style('font-size','12px')
            .style('color', isActive ? item.color : '#c9d1d9')
            .style('font-weight', isActive ? '600' : '400')
            .style('cursor','pointer').style('border-radius','5px')
            .style('background', isActive ? item.color + '18' : 'transparent')
            .style('display','flex').style('align-items','center').style('gap','6px')
            .html(`<span style="color:${item.color}">${item.label.split(' ')[0]}</span> ${item.label.split(' ').slice(1).join(' ')}${isActive ? ' <span style="margin-left:auto;font-size:10px;opacity:0.6">✓</span>' : ''}`)
            .on('mouseover', function() { if (!isActive) d3.select(this).style('background', '#ffffff10') })
            .on('mouseout',  function() { if (!isActive) d3.select(this).style('background', 'transparent') })
            .on('click', async () => {
              hideRoleMenu()
              try {
                await entityTypesApi.setDddRole(d.et_id, item.role)
                toast.success(`${d.et_name} → ${item.label}`)
                // Reload graph to reflect new role
                const newData = await graphApi.get(projectId)
                if (svgRef.current) drawGraph(newData as unknown as GraphData, svgRef.current)
              } catch (e) { toast.error(String(e)) }
            })
        })

        roleMenu
          .style('display','block')
          .style('left', (ev.clientX + 4) + 'px')
          .style('top',  (ev.clientY + 4) + 'px')
      })
      .on('click', function(ev, d) {
        ev.stopPropagation()
        if (d.is_default) return

        // Build neighbor set (clicked node + direct connections)
        const neighbors = new Set([d.id])
        validEdgeSels.forEach(e => {
          const s = typeof e.source === 'object' ? (e.source as ApiNode).id : e.source
          const t = typeof e.target === 'object' ? (e.target as ApiNode).id : e.target
          if (s === d.id) neighbors.add(t)
          if (t === d.id) neighbors.add(s)
        })

        // Toggle: if this node is already focused, restore all
        const alreadyFocused = d3.select(this).attr('data-focused') === 'true'
        if (alreadyFocused) {
          // Restore everything
          nodeSel.attr('data-focused', null)
            .transition().duration(200).style('opacity', null)
          edgeSel.transition().duration(200).style('opacity', null).style('stroke-opacity', null)
          hullPaths.transition().duration(200).style('opacity', null)
          etGroupPaths.transition().duration(200).style('opacity', null)
        } else {
          // Focus this node: dim non-neighbors
          nodeSel.attr('data-focused', null)
          d3.select(this).attr('data-focused', 'true')
          nodeSel.transition().duration(220)
            .style('opacity', (n: ApiNode) => neighbors.has(n.id) ? '1' : '0.07')
          edgeSel.transition().duration(220)
            .style('opacity', (e: typeof simEdges[0]) => {
              const s = typeof e.source === 'object' ? (e.source as ApiNode).id : e.source
              const t = typeof e.target === 'object' ? (e.target as ApiNode).id : e.target
              return (neighbors.has(s) && neighbors.has(t)) ? '1' : '0.04'
            })
          hullPaths.transition().duration(220).style('opacity', '0.12')
          etGroupPaths.transition().duration(220).style('opacity', '0.12')
        }
      })

    svg.on('click', () => {
      // Background click → restore all
      nodeSel.attr('data-focused', null).transition().duration(200).style('opacity', null)
      edgeSel.transition().duration(200).style('opacity', null)
      hullPaths.transition().duration(200).style('opacity', null)
      etGroupPaths.transition().duration(200).style('opacity', null)
    })

    // Legend (fixed to SVG top-right)
    const legendBg = svg.append('g').attr('transform', `translate(${W - 200},10)`)
    legendBg.append('rect').attr('width', 188).attr('height', 155)
      .attr('fill','#161b22').attr('fill-opacity', 0.85)
      .attr('stroke','#30363d').attr('stroke-width', 1).attr('rx', 8)
    const legend = legendBg.append('g').attr('transform','translate(12,12)')

    // DDD ROLE section — ring-based indicators
    legend.append('text').attr('y', 0).attr('font-size', 10).attr('fill','#8b949e').attr('font-weight', 600).attr('letter-spacing','1px').text('DDD ROLE')
    ;([
      { label:'Aggregate Root', etC:'#94a3b8', ring:'#f85149', r:8, ringW:2.5, ringDash: null as string|null, glow: true  },
      { label:'Entity',         etC:'#94a3b8', ring:'none',    r:6, ringW:0,   ringDash: null as string|null, glow: false },
      { label:'Value Object',   etC:'#94a3b8', ring:'#3fb950', r:5, ringW:1.5, ringDash:'3,3' as string|null, glow: false },
    ]).forEach((item, i) => {
      const y = 16 + i * 22
      const cx = 10, cy = y + 6
      // Glow halo for AR
      if (item.glow) {
        legend.append('circle').attr('cx', cx).attr('cy', cy).attr('r', item.r + 5)
          .attr('fill', item.ring + '18').attr('stroke', item.ring).attr('stroke-width', 0.5).attr('stroke-opacity', 0.4)
      }
      // Main circle
      legend.append('circle').attr('cx', cx).attr('cy', cy).attr('r', item.r)
        .attr('fill', item.etC + (item.glow ? '48' : '28')).attr('stroke', item.etC).attr('stroke-width', item.glow ? 2 : 1.5)
      // DDD ring
      if (item.ring !== 'none') {
        legend.append('circle').attr('cx', cx).attr('cy', cy).attr('r', item.r + 2)
          .attr('fill','none').attr('stroke', item.ring)
          .attr('stroke-width', item.ringW).attr('stroke-dasharray', item.ringDash)
      }
      // ◆ badge for AR
      if (item.glow) {
        legend.append('text').attr('x', cx).attr('y', cy - item.r + 4)
          .attr('font-size', 6).attr('text-anchor','middle').attr('fill', item.ring).attr('font-weight', 700).text('◆')
      }
      legend.append('text').attr('x', 28).attr('y', y + 10).attr('font-size', 10).attr('fill','#c9d1d9').text(item.label)
    })

    // ET COLOR note
    legend.append('text').attr('y', 88).attr('font-size', 9).attr('fill','#6e7681').attr('font-style','italic').text('节点颜色 = ET 类型颜色')

    // RELATIONSHIP section
    legend.append('text').attr('y', 106).attr('font-size', 10).attr('fill','#8b949e').attr('font-weight', 600).attr('letter-spacing','1px').text('RELATIONSHIP')
    ;([
      { label:'Intra-BC', color:'#388bfd', dash: null as string | null },
      { label:'Cross-BC', color:'#f97316', dash:'6,3' as string | null },
    ]).forEach((item, i) => {
      const y = 122 + i * 18
      legend.append('line').attr('x1', 0).attr('y1', y).attr('x2', 16).attr('y2', y)
        .attr('stroke', item.color).attr('stroke-width', 1.5).attr('stroke-dasharray', item.dash)
      legend.append('text').attr('x', 22).attr('y', y + 4).attr('font-size', 10).attr('fill','#c9d1d9').text(item.label)
    })

    // Hull update function
    const nodeSet = new Map(simNodes.map(n => [n.id, n]))
    function updateHulls() {
      // ET-type group hulls (always shown, very subtle)
      etGroupPaths.each(function(grp) {
        const members = grp.nodeIds.map(id => nodeSet.get(id)).filter((n): n is ApiNode & { x: number; y: number } => n != null && n.x != null)
        if (members.length < 2) { d3.select(this).attr('d', null); return }
        const pad = R + 14
        const pts = members.flatMap(n => [[n.x-pad, n.y-pad],[n.x+pad, n.y-pad],[n.x+pad, n.y+pad],[n.x-pad, n.y+pad]] as [number,number][])
        const hull = d3.polygonHull(pts)
        if (hull) d3.select(this).attr('d', 'M' + hull.map(p => p.join(',')).join('L') + 'Z')
      })
      etGroupLabels.each(function(grp) {
        const members = grp.nodeIds.map(id => nodeSet.get(id)).filter((n): n is ApiNode & { x: number; y: number } => n != null && n.x != null)
        if (!members.length) return
        const pad = R + 14
        d3.select(this)
          .attr('x', d3.mean(members, n => n.x)!)
          .attr('y', d3.min(members, n => n.y)! - pad - 4)
      })

      hullPaths.each(function(bc) {
        const members = bc.ids.map(id => nodeSet.get(id)).filter((n): n is ApiNode & { x: number; y: number } => n != null && n.x != null)
        if (!members.length) return
        bcCenters[bc.id] = { x: d3.mean(members, n => n.x)!, y: d3.mean(members, n => n.y)! }
        const pad = bc.isFold ? R + 44 : R + 18
        const r   = Math.round(R * 1.45) + pad
        const pts = members.flatMap(n => [[n.x-r, n.y-r],[n.x+r, n.y-r],[n.x+r, n.y+r],[n.x-r, n.y+r]] as [number,number][])
        const hull = d3.polygonHull(pts)
        if (hull) d3.select(this).attr('d', 'M' + hull.map(p => p.join(',')).join('L') + 'Z')
      })
      hullLabels.each(function(bc) {
        const members = bc.ids.map(id => nodeSet.get(id)).filter((n): n is ApiNode & { x: number; y: number } => n != null && n.x != null)
        if (!members.length) return
        const pad = bc.isFold ? R + 44 : R + 18
        d3.select(this)
          .attr('x', d3.mean(members, n => n.x)!)
          .attr('y', d3.min(members, n => n.y)! - pad - 8)
          .attr('text-anchor','middle')
      })
    }

    // Force simulation
    simRef.current = d3.forceSimulation<ApiNode>(simNodes)
      .alphaDecay(0.012)
      .velocityDecay(0.3)
      .force('link', d3.forceLink<ApiNode, typeof simEdges[0]>(validEdgeSels)
        .id(d => d.id)
        .distance(d => {
          const s = typeof d.source === 'object' ? d.source as ApiNode : simNodes.find(n => n.id === d.source)
          const t = typeof d.target === 'object' ? d.target as ApiNode : simNodes.find(n => n.id === d.target)
          return s && t && s.et_id === t.et_id ? 40 : 60
        })
        .strength(d => {
          const s = typeof d.source === 'object' ? d.source as ApiNode : simNodes.find(n => n.id === d.source)
          const t = typeof d.target === 'object' ? d.target as ApiNode : simNodes.find(n => n.id === d.target)
          return s && t && s.et_id === t.et_id ? 0.5 : 0.08
        })
      )
      .force('charge',  d3.forceManyBody<ApiNode>().strength(d => d.is_default ? -8 : -220))
      .force('collide', d3.forceCollide<ApiNode>(d => d.is_default ? 6 : nodeR(d) + 6))
      .force('bcX', d3.forceX<ApiNode>(d => d.is_default ? (d.x ?? W/2) : (etCentroids[d.et_id] ?? {x:W/2}).x).strength(d => d.is_default ? 0.01 : 0.18))
      .force('bcY', d3.forceY<ApiNode>(d => d.is_default ? (d.y ?? H/2) : (etCentroids[d.et_id] ?? {y:H/2}).y).strength(d => d.is_default ? 0.01 : 0.18))
      .on('tick', () => {
        edgeSel
          .attr('x1', d => (d.source as ApiNode).x ?? 0)
          .attr('y1', d => (d.source as ApiNode).y ?? 0)
          .attr('x2', d => {
            const src = d.source as ApiNode; const tgt = d.target as ApiNode
            const dx = (tgt.x ?? 0) - (src.x ?? 0), dy = (tgt.y ?? 0) - (src.y ?? 0)
            const dist = Math.sqrt(dx*dx + dy*dy) || 1
            return (tgt.x ?? 0) - (dx / dist) * (nodeR(tgt) + 4)
          })
          .attr('y2', d => {
            const src = d.source as ApiNode; const tgt = d.target as ApiNode
            const dx = (tgt.x ?? 0) - (src.x ?? 0), dy = (tgt.y ?? 0) - (src.y ?? 0)
            const dist = Math.sqrt(dx*dx + dy*dy) || 1
            return (tgt.y ?? 0) - (dy / dist) * (nodeR(tgt) + 4)
          })

        nodeSel.attr('transform', d => `translate(${d.x ?? 0},${d.y ?? 0})`)
        updateHulls()

        if (bcLinks.length) {
          bcLinkSel.attr('d', cl => {
            const a = bcCenters[cl.from] || bcCentroids[cl.from] || {x:W/2,y:H/2}
            const b = bcCenters[cl.to]   || bcCentroids[cl.to]   || {x:W/2,y:H/2}
            const dx = b.x-a.x, dy = b.y-a.y, len = Math.hypot(dx,dy)||1
            const nx = -dy/len, ny = dx/len, off = cl._offset ?? 0
            const mx = (a.x+b.x)/2 + nx*off, my = (a.y+b.y)/2 + ny*off
            return `M${a.x},${a.y} Q${mx},${my} ${b.x},${b.y}`
          })
          bcLinkLabels
            .attr('x', cl => {
              const a = bcCenters[cl.from] || bcCentroids[cl.from] || {x:W/2,y:H/2}
              const b = bcCenters[cl.to]   || bcCentroids[cl.to]   || {x:W/2,y:H/2}
              const dx=b.x-a.x,dy=b.y-a.y,len=Math.hypot(dx,dy)||1
              return (a.x+b.x)/2 + (-dy/len)*(cl._offset??0)
            })
            .attr('y', cl => {
              const a = bcCenters[cl.from] || bcCentroids[cl.from] || {x:W/2,y:H/2}
              const b = bcCenters[cl.to]   || bcCentroids[cl.to]   || {x:W/2,y:H/2}
              const dx=b.x-a.x,dy=b.y-a.y,len=Math.hypot(dx,dy)||1
              return (a.y+b.y)/2 + (dx/len)*(cl._offset??0) - 6
            })
        }
      })
  }

  return (
    <div ref={containerRef} className="flex flex-col h-full overflow-hidden bg-slate-950">
      <Controls
        frozen={frozen} labels={labels} hulls={hulls}
        onFreeze={handleFreeze} onLabels={handleLabels} onHulls={handleHulls}
        onFullscreen={handleFullscreen}
        onAutoLink={handleAutoLink} linking={linking}
        loading={loading}
      />
      <div className="flex-1 overflow-hidden">
        <svg ref={svgRef} className="w-full h-full block" />
      </div>
    </div>
  )
}
