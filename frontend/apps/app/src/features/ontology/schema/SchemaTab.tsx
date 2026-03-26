import { useEffect } from 'react'
import { entityTypesApi } from '@/api'
import { useSchemaStore } from '@/store/schemaStore'
import EntityTypeList from './EntityTypeList'
import EntityTypeDetail from './EntityTypeDetail'

export default function SchemaTab() {
  const { setEntityTypes } = useSchemaStore()

  useEffect(() => {
    entityTypesApi.list().then(setEntityTypes).catch(console.error)
  }, [])

  return (
    <div className="flex h-full overflow-hidden">
      {/* Left: ET list */}
      <div className="w-60 flex-shrink-0 border-r border-slate-800 flex flex-col overflow-hidden">
        <EntityTypeList />
      </div>

      {/* Right: ET detail */}
      <div className="flex-1 overflow-y-auto">
        <EntityTypeDetail />
      </div>
    </div>
  )
}
