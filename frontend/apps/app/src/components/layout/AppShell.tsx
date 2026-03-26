import { Outlet, NavLink, Link, useNavigate, useLocation, useParams } from 'react-router-dom'
import { cn } from '@/lib/utils'

export default function AppShell() {
  const location   = useLocation()
  const navigate   = useNavigate()
  const { projectId } = useParams<{ projectId?: string }>()

  const isOntology  = location.pathname === '/ontology'
  const isWorkspace = location.pathname.startsWith('/project/')

  return (
    <div className="flex flex-col h-screen overflow-hidden">
      {/* Top nav */}
      <header className="flex items-center gap-3 px-4 h-12 border-b border-slate-800 flex-shrink-0 bg-slate-950">
        <button
          onClick={() => navigate('/')}
          className="text-indigo-400 font-bold text-sm tracking-wide hover:text-indigo-300 transition-colors flex-shrink-0"
        >
          Palantir
        </button>

        {/* Breadcrumb */}
        {isWorkspace && (
          <>
            <span className="text-slate-700 text-xs">›</span>
            <Link to="/" className="text-xs text-slate-500 hover:text-slate-300 transition-colors">项目</Link>
            <span className="text-slate-700 text-xs">›</span>
            <span className="text-xs text-slate-300 font-medium">{projectId?.slice(0, 8)}…</span>
            <span className="text-slate-700 text-xs mx-1">|</span>
            <NavLink
              to={`/ontology`}
              className={({ isActive }) =>
                cn('text-xs transition-colors', isActive ? 'text-slate-200' : 'text-slate-500 hover:text-slate-300')
              }
            >
              Ontology →
            </NavLink>
          </>
        )}

        {isOntology && (
          <>
            <span className="text-slate-700 text-xs">›</span>
            <NavLink to="/" className={({ isActive }) =>
              cn('text-xs transition-colors', isActive ? 'text-slate-200' : 'text-slate-500 hover:text-slate-300')}>
              项目
            </NavLink>
            <span className="text-slate-700 text-xs">›</span>
            <span className="text-xs text-slate-300 font-medium">Ontology</span>
          </>
        )}

        {!isWorkspace && !isOntology && (
          <>
            <span className="text-slate-700 text-xs">›</span>
            <span className="text-xs text-slate-400">项目</span>
          </>
        )}

        {/* Right: Ontology quick-nav when on workspace */}
        {isWorkspace && (
          <div className="ml-auto flex items-center gap-1">
            {(['import', 'schema', 'browse', 'graph'] as const).map(tab => (
              <NavLink
                key={tab}
                to={`/ontology?tab=${tab}`}
                className={({ isActive }) =>
                  cn('px-3 py-1 rounded-lg text-xs font-medium transition-colors',
                    isActive ? 'bg-indigo-600 text-white' : 'text-slate-500 hover:text-slate-300 hover:bg-slate-800')
                }
              >
                {{ import:'导入', schema:'模型', browse:'浏览', graph:'图谱' }[tab]}
              </NavLink>
            ))}
          </div>
        )}
      </header>

      {/* Content */}
      <main className="flex-1 overflow-hidden">
        <Outlet />
      </main>
    </div>
  )
}
