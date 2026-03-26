import { useEffect, useState } from 'react'
import { Outlet, Link, useNavigate, useLocation, useParams } from 'react-router-dom'
import { projectsApi } from '@/api'

export default function AppShell() {
  const location  = useLocation()
  const navigate  = useNavigate()
  const { projectId } = useParams<{ projectId?: string }>()
  const [projectName, setProjectName] = useState<string | null>(null)

  const isWorkspace = location.pathname.startsWith('/project/')
  const isProjects  = location.pathname === '/'

  useEffect(() => {
    if (isWorkspace && projectId) {
      projectsApi.get(projectId).then(p => setProjectName(p.name)).catch(() => setProjectName(null))
    } else {
      setProjectName(null)
    }
  }, [projectId, isWorkspace])

  return (
    <div className="flex flex-col h-screen overflow-hidden">
      {/* Top nav */}
      <header className="flex items-center gap-2 px-4 h-12 border-b border-slate-800 flex-shrink-0 bg-slate-950">

        {/* Logo */}
        <button
          onClick={() => navigate('/')}
          className="text-indigo-400 font-bold text-sm tracking-wide hover:text-indigo-300 transition-colors flex-shrink-0 mr-1"
        >
          Palantir
        </button>

        {/* Breadcrumb */}
        <span className="text-slate-700 text-xs">›</span>

        {isProjects && (
          <span className="text-xs text-slate-400 font-medium">项目</span>
        )}

        {isWorkspace && (
          <>
            <Link to="/" className="text-xs text-slate-500 hover:text-slate-300 transition-colors">项目</Link>
            <span className="text-slate-700 text-xs">›</span>
            <span className="text-xs text-slate-300 font-medium truncate max-w-40">
              {projectName ?? projectId?.slice(0, 8) + '…'}
            </span>
          </>
        )}
      </header>

      {/* Content */}
      <main className="flex-1 overflow-hidden">
        <Outlet />
      </main>
    </div>
  )
}
