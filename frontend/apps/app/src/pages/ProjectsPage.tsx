import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { projectsApi, type Project } from '@/api'

export default function ProjectsPage() {
  const [projects, setProjects] = useState<Project[]>([])
  const [loading, setLoading] = useState(true)
  const navigate = useNavigate()

  useEffect(() => {
    projectsApi.list().then(setProjects).finally(() => setLoading(false))
  }, [])

  if (loading) return <div className="p-8 text-slate-500 text-sm">加载中…</div>

  return (
    <div className="p-8 max-w-2xl mx-auto">
      <h1 className="text-xl font-semibold text-slate-100 mb-6">项目</h1>
      <div className="space-y-2">
        {projects.map(p => (
          <button
            key={p.id}
            onClick={() => navigate(`/project/${p.id}`)}
            className="w-full text-left px-4 py-3 rounded-lg bg-slate-900 border border-slate-800 hover:border-indigo-700 hover:bg-slate-800 transition-colors"
          >
            <p className="text-sm font-medium text-slate-200">{p.name}</p>
            <p className="text-xs text-slate-500 mt-0.5">{p.id}</p>
          </button>
        ))}
        {projects.length === 0 && (
          <p className="text-slate-500 text-sm text-center py-12">暂无项目</p>
        )}
      </div>
    </div>
  )
}
