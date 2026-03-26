import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { toast } from 'sonner'
import { projectsApi, type Project } from '@/api'

function fmt(d: string) {
  return new Date(d).toLocaleDateString('zh-CN', { month: 'short', day: 'numeric', year: 'numeric' })
}

export default function ProjectsPage() {
  const [projects, setProjects] = useState<Project[]>([])
  const [loading,  setLoading]  = useState(true)
  const [creating, setCreating] = useState(false)
  const [newName,  setNewName]  = useState('')
  const [showForm, setShowForm] = useState(false)
  const navigate = useNavigate()

  useEffect(() => {
    projectsApi.list().then(setProjects).catch(console.error).finally(() => setLoading(false))
  }, [])

  async function handleCreate() {
    if (!newName.trim()) { toast.error('项目名称不能为空'); return }
    setCreating(true)
    try {
      const p = await projectsApi.create(newName.trim())
      setProjects(prev => [p, ...prev])
      setNewName('')
      setShowForm(false)
      toast.success(`项目「${p.name}」已创建`)
      navigate(`/project/${p.id}`)
    } catch (e) {
      toast.error(String(e))
    } finally {
      setCreating(false)
    }
  }

  if (loading) return <div className="p-10 text-slate-500 text-sm">加载中…</div>

  return (
    <div className="p-10 max-w-2xl mx-auto">
      {/* Header */}
      <div className="flex items-center justify-between mb-8">
        <div>
          <h1 className="text-2xl font-bold text-slate-100">项目</h1>
          <p className="text-sm text-slate-500 mt-1">
            {projects.length ? `共 ${projects.length} 个项目` : '暂无项目，新建一个开始'}
          </p>
        </div>
        <button
          onClick={() => { setShowForm(v => !v); setNewName('') }}
          className="px-4 py-2 text-sm bg-indigo-600 hover:bg-indigo-500 text-white rounded-lg transition-colors"
        >
          + 新建项目
        </button>
      </div>

      {/* Create form */}
      {showForm && (
        <div className="mb-6 p-4 bg-slate-900 border border-indigo-800/50 rounded-xl space-y-3">
          <p className="text-xs font-semibold text-slate-400 uppercase tracking-wider">新建项目</p>
          <div className="flex gap-2">
            <input
              autoFocus
              value={newName}
              onChange={e => setNewName(e.target.value)}
              onKeyDown={e => { if (e.key === 'Enter') handleCreate(); if (e.key === 'Escape') setShowForm(false) }}
              placeholder="项目名称，如：电商平台 · 用户分析"
              className="flex-1 bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500"
            />
            <button
              onClick={handleCreate}
              disabled={creating}
              className="px-4 py-2 text-sm bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white rounded-lg transition-colors"
            >
              {creating ? '创建中…' : '创建'}
            </button>
            <button
              onClick={() => setShowForm(false)}
              className="px-4 py-2 text-sm border border-slate-700 text-slate-400 hover:bg-slate-800 rounded-lg transition-colors"
            >
              取消
            </button>
          </div>
        </div>
      )}

      {/* Project list */}
      {!projects.length && !showForm && (
        <div className="flex flex-col items-center justify-center py-20 text-center">
          <div className="text-5xl mb-4">🗂</div>
          <p className="text-slate-400 text-sm mb-2">还没有任何项目</p>
          <p className="text-slate-600 text-xs">点击「新建项目」开始</p>
        </div>
      )}

      <div className="space-y-2">
        {projects.map(p => (
          <button
            key={p.id}
            onClick={() => navigate(`/project/${p.id}`)}
            className="w-full text-left px-5 py-4 rounded-xl bg-slate-900 border border-slate-800 hover:border-indigo-700/60 hover:bg-slate-800/80 transition-all group"
          >
            <div className="flex items-center justify-between">
              <p className="text-sm font-semibold text-slate-200 group-hover:text-white transition-colors">{p.name}</p>
              <span className="text-xs text-slate-600 group-hover:text-slate-400 transition-colors">→</span>
            </div>
            <div className="flex items-center gap-3 mt-1.5">
              <p className="text-xs text-slate-600 font-mono">{p.id.slice(0, 8)}…</p>
              <span className="text-slate-700 text-xs">·</span>
              <p className="text-xs text-slate-600">{fmt(p.created_at)}</p>
            </div>
          </button>
        ))}
      </div>
    </div>
  )
}
