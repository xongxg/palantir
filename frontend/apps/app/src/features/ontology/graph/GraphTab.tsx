import { useEffect, useRef, useState } from 'react'
import * as d3 from 'd3'
import { graphApi } from '@/api'
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

// ── DDD colors ─────────────────────────────────────────────────────────────
const DDD_COLOR: Record<string, string> = {
  'aggregate-root': '#f85149',
  'entity':         '#388bfd',
  'value-object':   '#3fb950',
}
const ROLE_LABEL: Record<string, string> = {
  'aggregate-root': '◆ Aggregate Root',
  'entity':         '▸ Entity',
  'value-object':   '○ Value Object',
}

function escHtml(s: string) {
  return s.replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;')
}

// ── Controls bar ──────────────────────────────────────────────────────────
interface ControlsProps {
  frozen: boolean; labels: boolean; hulls: boolean
  onFreeze: () => void; onLabels: () => void; onHulls: () => void; onFullscreen: () => void
  loading: boolean
}
function Controls({ frozen, labels, hulls, onFreeze, onLabels, onHulls, onFullscreen, loading }: ControlsProps) {
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
        onClick={onFullscreen}
        className="ml-auto px-3 py-1.5 text-xs rounded-lg border bg-slate-800 border-slate-700 text-slate-400 hover:text-slate-200 transition-colors"
      >⛶ 全屏</button>
    </div>
  )
}

// ── Main component ─────────────────────────────────────────────────────────
export default function GraphTab() {
  const containerRef = useRef<HTMLDivElement>(null)
  const svgRef       = useRef<SVGSVGElement>(null)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const simRef       = useRef<d3.Simulation<any, any> | null>(null)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const hullLayerRef = useRef<d3.Selection<any, any, any, any> | null>(null)
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const labelSelRef  = useRef<d3.Selection<any, any, any, any> | null>(null)

  const [loading, setLoading]   = useState(true)
  const [frozen,  setFrozen]    = useState(false)
  const [labels,  setLabels]    = useState(true)
  const [hulls,   setHulls]     = useState(true)
  const frozenRef = useRef(false)

  // ── Draw graph ───────────────────────────────────────────────────────────
  useEffect(() => {
    let cancelled = false
    setLoading(true)

    graphApi.get().then(data => {
      if (cancelled) return
      setLoading(false)
      if (svgRef.current) drawGraph(data as unknown as GraphData, svgRef.current)
    }).catch(() => setLoading(false))

    return () => { cancelled = true; simRef.current?.stop() }
  }, [])

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
  }

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
    hullLayerRef.current = null
    labelSelRef.current  = null

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
    const R = nodes.length > 80 ? 9 : nodes.length > 30 ? 13 : 17
    const nodeR = (d: ApiNode) => {
      if (d.is_default) return 2
      const role = etRoles[d.et_id] || 'entity'
      return role === 'aggregate-root' ? Math.round(R * 1.45) : role === 'entity' ? R : Math.round(R * 0.67)
    }
    const dddColor = (d: ApiNode) => DDD_COLOR[etRoles[d.et_id] || 'entity'] || '#388bfd'

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

    // Hull layer
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
      const color   = dddColor(d)
      const bc      = nodeBC[d.id] as BC | undefined
      const bcColor = bc?.color || '#475569'

      sel.append('circle').attr('class','node-hover-ring')
        .attr('r', r + 8).attr('fill','none')
        .attr('stroke', color).attr('stroke-width', 2).attr('stroke-opacity', 0).attr('pointer-events','none')
      sel.append('circle').attr('class','node-bc-ring')
        .attr('r', r + 3).attr('fill','none')
        .attr('stroke', bcColor).attr('stroke-width', 1).attr('stroke-opacity', 0.45).attr('pointer-events','none')
      sel.append('circle').attr('class','node-main')
        .attr('r', r)
        .attr('fill', color + (role === 'value-object' ? '20' : '2a'))
        .attr('stroke', color)
        .attr('stroke-width', role === 'aggregate-root' ? 2 : 1.5)
        .attr('stroke-dasharray', role === 'value-object' ? '4,3' : null)

      const lbl = (d.label || '').length > 14 ? d.label.slice(0, 13) + '…' : (d.label || '')
      sel.append('text').attr('class','graph-node-label')
        .attr('dy', r + 11).attr('font-size', 9)
        .attr('fill','#c9d1d9').attr('text-anchor','middle')
        .attr('paint-order','stroke').attr('stroke','#0d1117').attr('stroke-width', 3).attr('stroke-opacity', 0.85)
        .text(lbl)
      sel.append('text').attr('class','graph-node-label')
        .attr('dy', r + 22).attr('font-size', 8)
        .attr('fill', color).attr('opacity', 0.6).attr('text-anchor','middle')
        .attr('paint-order','stroke').attr('stroke','#0d1117').attr('stroke-width', 2).attr('stroke-opacity', 0.85)
        .text(d.et_name || '')
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

    nodeSel
      .on('mouseenter', (ev, d) => {
        if (d.is_default) return
        const gEl = d3.select<SVGGElement, ApiNode>(ev.currentTarget)
        gEl.select('.node-main').transition().duration(150).ease(d3.easeBackOut.overshoot(1.5)).attr('transform','scale(1.18)')
        gEl.select('.node-hover-ring').transition().duration(200).attr('stroke-opacity', 0.45).attr('r', nodeR(d) + 10)

        const role    = etRoles[d.et_id] || 'entity'
        const col     = dddColor(d)
        const scalarEntries = Object.entries(d.props || {}).filter(([k]) => k !== 'id' && !k.endsWith('_id') && !k.endsWith('Id'))
        const fieldSection  = scalarEntries.map(([k, v]) =>
          `<div style="margin-bottom:1px"><span style="color:#6e7681">${escHtml(k)}:</span> <span style="color:#c9d1d9">${escHtml(String(v ?? ''))}</span></div>`
        ).join('')

        tip.html(
          `<div style="font-weight:600;color:${col};font-size:13px;margin-bottom:1px">${escHtml(d.label)}</div>` +
          `<div style="color:#8b949e;font-size:11px;margin-bottom:${fieldSection?'6px':'0'}">${escHtml(d.et_name)} · ${ROLE_LABEL[role] || role}</div>` +
          (fieldSection ? `<div style="font-size:11px;line-height:1.7;border-top:1px solid #30363d;padding-top:6px">${fieldSection}</div>` : '')
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
      .on('click', function(ev, d) {
        ev.stopPropagation()
        if (d.is_default) return
        const neighbors = new Set([d.id])
        validEdgeSels.forEach(e => {
          const s = typeof e.source === 'object' ? (e.source as ApiNode).id : e.source
          const t = typeof e.target === 'object' ? (e.target as ApiNode).id : e.target
          if (s === d.id) neighbors.add(t)
          if (t === d.id) neighbors.add(s)
        })
        const isHl = d3.select(this).classed('highlighted')
        nodeSel.classed('highlighted', false).classed('dimmed', !isHl)
        edgeSel.classed('dimmed', !isHl)
        hullPaths.classed('dimmed', !isHl)
        if (!isHl) {
          nodeSel.filter(n => neighbors.has(n.id)).classed('highlighted', true).classed('dimmed', false)
          edgeSel.filter(e => {
            const s = typeof e.source === 'object' ? (e.source as ApiNode).id : e.source
            const t = typeof e.target === 'object' ? (e.target as ApiNode).id : e.target
            return neighbors.has(s) && neighbors.has(t)
          }).classed('dimmed', false)
        }
      })

    svg.on('click', () => {
      nodeSel.classed('highlighted', false).classed('dimmed', false)
      edgeSel.classed('dimmed', false)
      hullPaths.classed('dimmed', false)
    })

    // Legend (fixed to SVG top-right)
    const legendBg = svg.append('g').attr('transform', `translate(${W - 190},10)`)
    legendBg.append('rect').attr('width', 178).attr('height', 130)
      .attr('fill','#161b22').attr('fill-opacity', 0.85)
      .attr('stroke','#30363d').attr('stroke-width', 1).attr('rx', 8)
    const legend = legendBg.append('g').attr('transform','translate(12,12)')
    legend.append('text').attr('y', 0).attr('font-size', 10).attr('fill','#8b949e').attr('font-weight', 600).attr('letter-spacing','1px').text('DDD CONCEPT')
    ;([
      { label:'Aggregate Root', color:'#f85149', r:7, strokeW:2 },
      { label:'Entity',         color:'#388bfd', r:6, strokeW:1.5 },
      { label:'Value Object',   color:'#3fb950', r:5, strokeW:1.5, dash:'4,3' },
    ]).forEach((item, i) => {
      const y = 16 + i * 20
      legend.append('circle').attr('cx', 8).attr('cy', y + 4).attr('r', item.r)
        .attr('fill', item.color + '28').attr('stroke', item.color)
        .attr('stroke-width', item.strokeW).attr('stroke-dasharray', item.dash || null)
      legend.append('text').attr('x', 20).attr('y', y + 8).attr('font-size', 10).attr('fill','#c9d1d9').text(item.label)
    })
    legend.append('text').attr('y', 78).attr('font-size', 10).attr('fill','#8b949e').attr('font-weight', 600).attr('letter-spacing','1px').text('RELATIONSHIP')
    ;([
      { label:'Intra-BC', color:'#388bfd', dash: null as string | null },
      { label:'Cross-BC', color:'#f97316', dash:'6,3' as string | null },
    ]).forEach((item, i) => {
      const y = 94 + i * 18
      legend.append('line').attr('x1', 0).attr('y1', y).attr('x2', 16).attr('y2', y)
        .attr('stroke', item.color).attr('stroke-width', 1.5).attr('stroke-dasharray', item.dash)
      legend.append('text').attr('x', 22).attr('y', y + 4).attr('font-size', 10).attr('fill','#c9d1d9').text(item.label)
    })

    // Hull update function
    const nodeSet = new Map(simNodes.map(n => [n.id, n]))
    function updateHulls() {
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
        loading={loading}
      />
      <div className="flex-1 overflow-hidden">
        <svg ref={svgRef} className="w-full h-full block" />
      </div>
    </div>
  )
}
