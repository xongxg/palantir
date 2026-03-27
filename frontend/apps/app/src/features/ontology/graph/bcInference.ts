// BC inference — ported from D3 ontology.html

export interface GNode { id: string; et_id: string; fold_id?: string; is_default?: boolean; x?: number; y?: number }
export interface GEdge { source: string | GNode; target: string | GNode; label?: string }
export interface GFold  { id: string; name: string }
export interface EtSummary { id: string; label: string; color: string; fold_id?: string; bc_id?: string; ddd_role?: string; count: number }

export interface BC {
  id: string
  name: string
  color: string
  ids: string[]   // node ids
  isFold: boolean
}

// Union-Find
function makeUF(ids: string[]) {
  const parent: Record<string, string> = {}
  ids.forEach(id => { parent[id] = id })
  function find(x: string): string {
    if (parent[x] !== x) parent[x] = find(parent[x])
    return parent[x]
  }
  function union(a: string, b: string) {
    parent[find(a)] = find(b)
  }
  return { find, union, parent }
}

// Detect child BCs within a fold via edge-density threshold
function detectChildBCs(
  nodeIds: string[],
  edges: GEdge[],
  threshold = 0.40,
): string[][] {
  if (nodeIds.length < 4) return [nodeIds]
  const idSet = new Set(nodeIds)
  const uf = makeUF(nodeIds)
  const edgeDensity = new Map<string, number>()

  edges.forEach(e => {
    const s = typeof e.source === 'object' ? e.source.id : e.source
    const t = typeof e.target === 'object' ? e.target.id : e.target
    if (!idSet.has(s) || !idSet.has(t) || s === t) return
    const key = [s, t].sort().join('|')
    edgeDensity.set(key, (edgeDensity.get(key) ?? 0) + 1)
  })

  edgeDensity.forEach((_, key) => {
    const [a, b] = key.split('|')
    const aNeighbors = [...idSet].filter(id => edgeDensity.has([id, a].sort().join('|')))
    const bNeighbors = [...idSet].filter(id => edgeDensity.has([id, b].sort().join('|')))
    const overlap = aNeighbors.filter(id => bNeighbors.includes(id)).length
    const union   = new Set([...aNeighbors, ...bNeighbors]).size
    const density = union > 0 ? overlap / union : 0
    if (density >= threshold) uf.union(a, b)
  })

  const groups: Record<string, string[]> = {}
  nodeIds.forEach(id => {
    const root = uf.find(id)
    ;(groups[root] ??= []).push(id)
  })
  return Object.values(groups).filter(g => g.length > 0)
}

const FOLD_PALETTE = [
  '#6366f1','#8b5cf6','#ec4899','#14b8a6','#f59e0b','#10b981','#3b82f6','#f97316',
]

export function buildBCsFromFolds(
  nodes: GNode[],
  folds: GFold[],
  etSummary: EtSummary[],
  edges: GEdge[],
): { bcs: BC[]; nodeBC: Record<string, BC> } {
  const bcs: BC[] = []
  const nodeBC: Record<string, BC> = {}

  // Phase 0: If any ETs have bc_id set, use server-side BC groupings directly
  // This takes priority over fold/client-side union-find
  const etToBcId = new Map<string, string>()
  etSummary.forEach(et => { if (et.bc_id) etToBcId.set(et.id, et.bc_id) })

  const bcGroups = new Map<string, string[]>()
  nodes.forEach(n => {
    if (n.is_default) return
    const bcId = etToBcId.get(n.et_id)
    if (bcId) {
      const grp = bcGroups.get(bcId) ?? []
      grp.push(n.id)
      bcGroups.set(bcId, grp)
    }
  })

  if (bcGroups.size > 0) {
    Array.from(bcGroups.entries()).forEach(([bcId, ids], gi) => {
      const color = FOLD_PALETTE[gi % FOLD_PALETTE.length]
      const bc: BC = { id: `sbc-${bcId}`, name: `BC ${gi + 1}`, color, ids, isFold: false }
      bcs.push(bc)
      ids.forEach(id => { nodeBC[id] = bc })
    })
    return { bcs, nodeBC }
  }

  // Phase 1: No server-side BCs — use fold grouping + client-side union-find
  if (!folds.length) {
    // No fold info — run union-find on ALL non-default nodes
    const allIds = nodes.filter(n => !n.is_default).map(n => n.id)
    if (allIds.length) {
      // Run union-find at ET-type level (not instance level) to avoid single-blob merging
      const nodeToET: Record<string, string> = {}
      nodes.forEach(n => { nodeToET[n.id] = n.et_id })
      const uniqueETs = [...new Set(allIds.map(id => nodeToET[id]).filter(Boolean))]

      // Build ET-level edges from instance edges
      const etEdgeSet = new Set<string>()
      edges.forEach(e => {
        const s = typeof e.source === 'object' ? e.source.id : e.source
        const t = typeof e.target === 'object' ? e.target.id : e.target
        const se = nodeToET[s], te = nodeToET[t]
        if (se && te && se !== te) etEdgeSet.add([se, te].sort().join('|'))
      })
      const etEdges: GEdge[] = [...etEdgeSet].map(k => { const [s, t] = k.split('|'); return { source: s, target: t } })

      const etGroups = detectChildBCs(uniqueETs, etEdges)
      if (etGroups.length > 1) {
        // Multiple ET-type groups → map instances back to their group
        const etToGroup: Record<string, number> = {}
        etGroups.forEach((grp, gi) => grp.forEach(etId => { etToGroup[etId] = gi }))
        const instanceGroups: string[][] = etGroups.map(() => [])
        allIds.forEach(id => {
          const gi = etToGroup[nodeToET[id]]
          if (gi !== undefined) instanceGroups[gi].push(id)
        })
        instanceGroups.forEach((grp, gi) => {
          if (!grp.length) return
          const color = FOLD_PALETTE[gi % FOLD_PALETTE.length]
          const bc: BC = { id: `bc-root-${gi}`, name: `BC ${gi + 1}`, color, ids: grp, isFold: false }
          bcs.push(bc)
          grp.forEach(id => { nodeBC[id] = bc })
        })
      }
      // If only 1 group — no hull (can't distinguish BCs without connections)
    }
    return { bcs, nodeBC }
  }

  folds.forEach((fold, fi) => {
    // Skip "default" fold — no hull needed
    if (fold.id === 'default' || fold.name === 'Default') return

    const foldColor = FOLD_PALETTE[fi % FOLD_PALETTE.length]
    const foldETs   = etSummary.filter(e => e.fold_id === fold.id).map(e => e.id)
    const foldNodes = nodes.filter(n => !n.is_default && foldETs.includes(n.et_id))
    if (!foldNodes.length) return

    // top-level fold BC hull
    const foldBC: BC = {
      id: `fold-${fold.id}`, name: fold.name, color: foldColor,
      ids: foldNodes.map(n => n.id), isFold: true,
    }
    bcs.push(foldBC)
    foldNodes.forEach(n => { nodeBC[n.id] = foldBC })

    // Prefer server-side bc_id groupings when available
    const serverBcIds = [...new Set(
      etSummary.filter(e => e.fold_id === fold.id && e.bc_id).map(e => e.bc_id!)
    )]

    if (serverBcIds.length > 1) {
      // Use server-confirmed BC groups
      serverBcIds.forEach((bcId, gi) => {
        const childColor = FOLD_PALETTE[(fi * 3 + gi + 2) % FOLD_PALETTE.length]
        const childEtIds = etSummary.filter(e => e.bc_id === bcId).map(e => e.id)
        const childNodes = foldNodes.filter(n => childEtIds.includes(n.et_id)).map(n => n.id)
        if (!childNodes.length) return
        const childBC: BC = {
          id: `sbc-${bcId}`,
          name: `BC ${gi + 1}`,
          color: childColor,
          ids: childNodes,
          isFold: false,
        }
        bcs.push(childBC)
        childNodes.forEach(id => { nodeBC[id] = childBC })
      })
    } else {
      // Fallback: client-side edge-density union-find
      const groups = detectChildBCs(foldNodes.map(n => n.id), edges)
      if (groups.length > 1) {
        // Multiple child BCs detected — draw each as a child hull
        groups.forEach((grp, gi) => {
          const childColor = FOLD_PALETTE[(fi * 3 + gi + 2) % FOLD_PALETTE.length]
          const childBC: BC = {
            id: `bc-${fold.id}-${gi}`,
            name: `${fold.name} · BC${gi + 1}`,
            color: childColor,
            ids: grp,
            isFold: false,
          }
          bcs.push(childBC)
          grp.forEach(id => { nodeBC[id] = childBC })
        })
      }
      // If only 1 group, fold hull already covers it — no duplicate child hull needed
    }
  })

  return { bcs, nodeBC }
}

export function classifyDDDRoles(
  etSummary: EtSummary[],
  _edges: GEdge[],
  _nodes: GNode[],
): Record<string, string> {
  const roles: Record<string, string> = {}
  // Role is inferred and persisted by the backend (infer_and_persist_ddd_roles).
  // VO is set explicitly by the user via the Model tab dropdown.
  etSummary.forEach(et => {
    roles[et.id] = (et.ddd_role ?? 'entity').replace('_', '-')
  })

  return roles
}
