import { Routes, Route, Navigate } from 'react-router-dom'
import { Toaster } from 'sonner'
import AppShell from '@/components/layout/AppShell'
import ProjectsPage from '@/pages/ProjectsPage'
import WorkspacePage from '@/pages/WorkspacePage'
import OntologyPage from '@/pages/OntologyPage'

export default function App() {
  return (
    <>
      <Routes>
        <Route path="/" element={<AppShell />}>
          <Route index element={<ProjectsPage />} />
          <Route path="project/:projectId" element={<WorkspacePage />} />
          <Route path="ontology" element={<OntologyPage />} />
          <Route path="*" element={<Navigate to="/" replace />} />
        </Route>
      </Routes>
      <Toaster theme="dark" position="bottom-right" richColors />
    </>
  )
}
