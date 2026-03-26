// BC inference — ported from D3 ontology.html

export interface GNode { id: string; et_id: string; fold_id?: string; is_default?: boolean; x?: number; y?: number }
export interface GEdge { source: string | GNode; target: string | GNode; label?: string }
export interface GFold  { id: string; name: string }
export interface EtSummary { id: string; label: string; color: string; fold_id?: string; count: number }

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

  if (!folds.length) {
    // no fold info — put all non-default nodes into one BC
    const allIds = nodes.filter(n => !n.is_default).map(n => n.id)
    if (allIds.length) {
      const bc: BC = { id: 'root', name: 'Default', color: FOLD_PALETTE[0], ids: allIds, isFold: true }
      bcs.push(bc)
      allIds.forEach(id => { nodeBC[id] = bc })
    }
    return { bcs, nodeBC }
  }

  folds.forEach((fold, fi) => {
    const foldColor = FOLD_PALETTE[fi % FOLD_PALETTE.length]
    const foldETs   = etSummary.filter(e => e.fold_id === fold.id).map(e => e.id)
    const foldNodes = nodes.filter(n => !n.is_default && foldETs.includes(n.et_id))
    if (!foldNodes.length) return

    // top-level fold BC
    const foldBC: BC = {
      id: `fold-${fold.id}`, name: fold.name, color: foldColor,
      ids: foldNodes.map(n => n.id), isFold: true,
    }
    bcs.push(foldBC)
    foldNodes.forEach(n => { nodeBC[n.id] = foldBC })

    // child BCs via edge density
    const groups = detectChildBCs(foldNodes.map(n => n.id), edges)
    if (groups.length > 1) {
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
  })

  return { bcs, nodeBC }
}

export function classifyDDDRoles(
  etSummary: EtSummary[],
  edges: GEdge[],
  nodes: GNode[],
): Record<string, string> {
  const roles: Record<string, string> = {}
  const etIds = etSummary.map(e => e.id)

  // Count outgoing edges per ET
  const outCount: Record<string, number> = {}
  const inCount:  Record<string, number> = {}
  etIds.forEach(id => { outCount[id] = 0; inCount[id] = 0 })

  const nodeToET = Object.fromEntries(nodes.map(n => [n.id, n.et_id]))
  edges.forEach(e => {
    const s = typeof e.source === 'object' ? e.source.id : e.source
    const t = typeof e.target === 'object' ? e.target.id : e.target
    const setId = nodeToET[s]
    const tetId = nodeToET[t]
    if (setId) outCount[setId] = (outCount[setId] ?? 0) + 1
    if (tetId) inCount[tetId]  = (inCount[tetId]  ?? 0) + 1
  })

  etIds.forEach(id => {
    const out = outCount[id] ?? 0
    const inn = inCount[id]  ?? 0
    const et  = etSummary.find(e => e.id === id)
    if (et?.label?.toLowerCase().includes('event') || et?.label?.toLowerCase().includes('evt')) {
      roles[id] = 'value-object'
    } else if (out >= 3 || (out > inn && out >= 2)) {
      roles[id] = 'aggregate-root'
    } else if (inn >= 2 && out <= 1) {
      roles[id] = 'value-object'
    } else {
      roles[id] = 'entity'
    }
  })

  return roles
}
