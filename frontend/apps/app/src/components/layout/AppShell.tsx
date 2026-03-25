import { Outlet, NavLink, useNavigate } from 'react-router-dom'
import { cn } from '@/lib/utils'

const NAV_TABS = [
  { label: '导入', path: '/ontology?tab=import' },
  { label: '模型', path: '/ontology?tab=schema' },
  { label: '浏览', path: '/ontology?tab=browse' },
  { label: '图谱', path: '/ontology?tab=graph' },
]

export default function AppShell() {
  const navigate = useNavigate()

  return (
    <div className="flex flex-col h-screen overflow-hidden">
      {/* Top nav */}
      <header className="flex items-center gap-4 px-4 h-12 border-b border-slate-800 flex-shrink-0 bg-slate-950">
        <button
          onClick={() => navigate('/')}
          className="text-indigo-400 font-bold text-sm tracking-wide hover:text-indigo-300 transition-colors"
        >
          Palantir
        </button>
        <NavLink
          to="/"
          className={({ isActive }) =>
            cn('text-xs text-slate-400 hover:text-slate-200 transition-colors', isActive && 'text-slate-200')
          }
        >
          ← 返回项目
        </NavLink>
        <span className="text-slate-600 text-xs">|</span>
        <span className="text-slate-200 text-sm font-medium">Ontology</span>

        <div className="ml-auto flex items-center gap-1">
          {NAV_TABS.map(tab => (
            <NavLink
              key={tab.label}
              to={tab.path}
              className={({ isActive }) =>
                cn(
                  'px-4 py-1.5 rounded-lg text-xs font-medium transition-colors',
                  isActive
                    ? 'bg-indigo-600 text-white'
                    : 'text-slate-400 hover:text-slate-200 hover:bg-slate-800',
                )
              }
            >
              {tab.label}
            </NavLink>
          ))}
        </div>
      </header>

      {/* Content */}
      <main className="flex-1 overflow-hidden">
        <Outlet />
      </main>
    </div>
  )
}
