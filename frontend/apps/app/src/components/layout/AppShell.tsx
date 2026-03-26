import { useEffect, useState } from 'react'
import { Outlet, NavLink, Link, useNavigate, useLocation, useParams } from 'react-router-dom'
import { cn } from '@/lib/utils'
import { projectsApi } from '@/api'

const ONTOLOGY_TABS = [
  { key: 'import', label: '导入' },
  { key: 'schema', label: '模型' },
  { key: 'browse', label: '浏览' },
  { key: 'graph',  label: '图谱' },
] as const

export default function AppShell() {
  const location  = useLocation()
  const navigate  = useNavigate()
  const { projectId } = useParams<{ projectId?: string }>()
  const [projectName, setProjectName] = useState<string | null>(null)

  const isWorkspace = location.pathname.startsWith('/project/')
  const isOntology  = location.pathname === '/ontology'
  const isProjects  = location.pathname === '/'

  // Load project name when on workspace
  useEffect(() => {
    if (isWorkspace && projectId) {
      projectsApi.get(projectId).then(p => setProjectName(p.name)).catch(() => setProjectName(null))
    } else {
      setProjectName(null)
    }
  }, [projectId, isWorkspace])

  // Active Ontology tab from search params
  const searchTab = new URLSearchParams(location.search).get('tab') ?? 'import'

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

        {isOntology && (
          <>
            <Link to="/" className="text-xs text-slate-500 hover:text-slate-300 transition-colors">项目</Link>
            <span className="text-slate-700 text-xs">›</span>
            <span className="text-xs text-slate-300 font-medium">Ontology</span>
          </>
        )}

        {/* Right side */}
        <div className="ml-auto flex items-center gap-1">
          {/* Ontology tab quick-nav — always visible */}
          {(isWorkspace || isOntology) && ONTOLOGY_TABS.map(t => (
            <NavLink
              key={t.key}
              to={`/ontology?tab=${t.key}`}
              className={cn(
                'px-3 py-1 rounded-lg text-xs font-medium transition-colors',
                isOntology && searchTab === t.key
                  ? 'bg-indigo-600 text-white'
                  : 'text-slate-500 hover:text-slate-300 hover:bg-slate-800',
              )}
            >
              {t.label}
            </NavLink>
          ))}

          {/* Workspace link when on Ontology */}
          {isOntology && projectId && (
            <>
              <span className="text-slate-700 text-xs mx-1">|</span>
              <Link
                to={`/project/${projectId}`}
                className="text-xs text-slate-500 hover:text-slate-300 transition-colors"
              >
                ← 返回工作区
              </Link>
            </>
          )}
        </div>
      </header>

      {/* Content */}
      <main className="flex-1 overflow-hidden">
        <Outlet />
      </main>
    </div>
  )
}
