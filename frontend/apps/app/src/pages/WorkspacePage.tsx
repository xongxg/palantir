import { useParams } from 'react-router-dom'

export default function WorkspacePage() {
  const { projectId } = useParams()
  return (
    <div className="p-8 text-slate-400 text-sm">
      Workspace — project: {projectId}
      <p className="mt-2 text-slate-600">迁移中…</p>
    </div>
  )
}
