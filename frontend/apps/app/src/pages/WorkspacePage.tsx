import { useEffect, useRef, useState } from 'react'
import { useParams, useNavigate, useSearchParams } from 'react-router-dom'
import { toast } from 'sonner'
import { projectsApi, sourcesApi, entityTypesApi } from '@/api'
import type { DataSource, Dataset, SyncJob, EntityType, Project } from '@/api'
import { cn } from '@/lib/utils'
import ImportTab from '@/features/ontology/import/ImportTab'
import SchemaTab from '@/features/ontology/schema/SchemaTab'
import BrowseTab from '@/features/ontology/browse/BrowseTab'
import GraphTab from '@/features/ontology/graph/GraphTab'

// ── Source type icons / labels ─────────────────────────────────────────────
const SRC_ICON: Record<string, string> = { s3:'🪣', csv:'📄', db:'🗄', rest:'🌐' }
const SRC_LABEL: Record<string, string> = {
  s3: '☁ Cloud Storage (S3 / OSS / RustFS)',
  csv: '📄 CSV 文件',
  db: '🗄 数据库 (MySQL / PG / SQLite)',
  rest: '🌐 REST API',
}
const STATUS_DOT: Record<string, string> = {
  idle:    'bg-slate-600',
  syncing: 'bg-yellow-400 animate-pulse',
  synced:  'bg-emerald-400',
  error:   'bg-red-500',
}

// ── Source form config fields ──────────────────────────────────────────────
interface S3Config { provider?: string; endpoint?: string; bucket?: string; prefix?: string; access_key?: string; secret_key?: string; pem_cert?: string; selected_files?: string[] }
const PROVIDER_DEFAULTS: Record<string, string> = {
  aws:'', aliyun:'', tencent:'', huawei:'', minio:'https://play.min.io', rustfs:'http://localhost:9000',
}

// ── S3 form ────────────────────────────────────────────────────────────────
function S3Form({ cfg, selectedFiles }: {
  cfg: S3Config
  selectedFiles: Set<string>
}) {
  const [provider,   setProvider]   = useState(cfg.provider   || 'rustfs')
  const [endpoint,   setEndpoint]   = useState(cfg.endpoint   || 'http://localhost:9000')
  const [bucket,     setBucket]     = useState(cfg.bucket     || 'mybucket1')
  const [prefix,     setPrefix]     = useState(cfg.prefix     || '')
  const [accessKey,  setAccessKey]  = useState(cfg.access_key || '')
  const [secretKey,  setSecretKey]  = useState(cfg.secret_key || '')
  const [pemCert,    setPemCert]    = useState(cfg.pem_cert   || '')

  // expose current values via hidden inputs with data attrs
  return (
    <div className="contents" data-s3>
      <div className="col-span-2 text-[11px] font-semibold text-slate-500 uppercase tracking-wider pt-3 border-t border-slate-800 mt-1">云存储配置</div>
      <div>
        <label className="text-xs text-slate-400 block mb-1">云服务商</label>
        <select
          name="provider" value={provider}
          onChange={e => { setProvider(e.target.value); setEndpoint(PROVIDER_DEFAULTS[e.target.value] ?? '') }}
          className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500"
        >
          {Object.entries({ aws:'AWS S3', aliyun:'阿里云 OSS', tencent:'腾讯云 COS', huawei:'华为云 OBS', minio:'MinIO（自定义）', rustfs:'RustFS（自定义）' }).map(([v,l]) => (
            <option key={v} value={v}>{l}</option>
          ))}
        </select>
      </div>
      <div>
        <label className="text-xs text-slate-400 block mb-1">Bucket *</label>
        <input name="bucket" value={bucket} onChange={e => setBucket(e.target.value)} placeholder="mybucket1"
          className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500" />
      </div>
      <div className="col-span-2">
        <label className="text-xs text-slate-400 block mb-1">Endpoint（MinIO / RustFS / 私有化）</label>
        <input name="endpoint" value={endpoint} onChange={e => setEndpoint(e.target.value)} placeholder="http://localhost:9000"
          className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500" />
      </div>
      <div>
        <label className="text-xs text-slate-400 block mb-1">路径前缀（可选）</label>
        <input name="prefix" value={prefix} onChange={e => setPrefix(e.target.value)} placeholder="data/hr/"
          className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500" />
      </div>
      <div>
        <label className="text-xs text-slate-400 block mb-1">Access Key *</label>
        <input name="access_key" value={accessKey} onChange={e => setAccessKey(e.target.value)} placeholder="AKIAIOSFODNN7EXAMPLE"
          className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500" />
      </div>
      <div>
        <label className="text-xs text-slate-400 block mb-1">Secret Key *</label>
        <input name="secret_key" type="password" value={secretKey} onChange={e => setSecretKey(e.target.value)} placeholder="••••••••"
          className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500" />
      </div>
      <div className="col-span-2">
        <label className="text-xs text-slate-400 block mb-1">SSL/PEM 证书（可选）</label>
        <textarea name="pem_cert" value={pemCert} onChange={e => setPemCert(e.target.value)}
          placeholder="-----BEGIN CERTIFICATE-----&#10;..." rows={3}
          className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500 resize-y" />
      </div>
      {selectedFiles.size > 0 && (
        <div className="col-span-2 px-3 py-2 rounded-lg text-xs text-indigo-300" style={{ background: 'rgba(99,102,241,.1)', border: '1px solid #4f46e5' }}>
          已选 {selectedFiles.size} 个文件：{[...selectedFiles].map(f => f.split('/').pop()).join('、')}
        </div>
      )}
      {/* Hidden inputs to pass values up via form ref */}
      <input type="hidden" name="_provider"   value={provider} />
      <input type="hidden" name="_endpoint"   value={endpoint} />
      <input type="hidden" name="_bucket"     value={bucket} />
      <input type="hidden" name="_prefix"     value={prefix} />
      <input type="hidden" name="_access_key" value={accessKey} />
      <input type="hidden" name="_secret_key" value={secretKey} />
      <input type="hidden" name="_pem_cert"   value={pemCert} />
    </div>
  )
}

// ── Datasets panel ─────────────────────────────────────────────────────────
function DatasetsPanel({ srcId, projectId }: { srcId: string; projectId: string }) {
  const [datasets,     setDatasets]     = useState<Dataset[]>([])
  const [entityTypes,  setEntityTypes]  = useState<EntityType[]>([])
  const [loading,      setLoading]      = useState(true)
  const navigate = useNavigate()

  useEffect(() => {
    Promise.all([
      sourcesApi.datasets(srcId),
      entityTypesApi.list(),
    ]).then(([ds, ets]) => { setDatasets(ds); setEntityTypes(ets) })
      .catch(console.error)
      .finally(() => setLoading(false))
  }, [srcId])

  if (loading) return <p className="text-xs text-slate-500">加载中…</p>
  if (!datasets.length) return <p className="text-xs text-slate-500">暂无 Dataset</p>

  const etMap = Object.fromEntries(entityTypes.map(e => [e.id, e.display_name || e.name]))

  return (
    <div className="space-y-2">
      {datasets.map(ds => {
        const hasData = (ds.record_count ?? 0) > 0
        const etName  = ds.entity_type_id ? (etMap[ds.entity_type_id] || ds.entity_type_id.slice(0,8)+'…') : null
        const isMapped = !!etName

        return (
          <div key={ds.id} className={cn(
            'flex items-center gap-3 px-3 py-2.5 rounded-lg border',
            isMapped ? 'bg-slate-900 border-blue-900/50' : hasData ? 'bg-slate-900 border-indigo-900/50' : 'bg-slate-900 border-slate-800',
          )}>
            <div className="flex-1 min-w-0">
              <p className="text-sm font-medium text-slate-200 truncate">{ds.name || ds.id}</p>
              <div className="flex items-center gap-2 mt-1">
                {isMapped
                  ? <span className="text-[10px] px-1.5 py-0.5 rounded text-blue-300" style={{ background: 'rgba(59,130,246,.12)' }}>🔗 {etName}</span>
                  : hasData
                  ? <span className="text-[10px] px-1.5 py-0.5 rounded text-indigo-300" style={{ background: 'rgba(99,102,241,.1)' }}>📋 Schema 可推导</span>
                  : <span className="text-[10px] px-1.5 py-0.5 rounded text-slate-500" style={{ background: 'rgba(100,116,139,.1)' }}>⚠ 暂无数据</span>
                }
                <span className="text-[11px] text-slate-600">{ds.record_count ?? 0} 条</span>
              </div>
            </div>
            <button
              onClick={() => navigate(`/project/${projectId}?tab=import&dataset=${ds.id}`)}
              className={cn(
                'text-xs px-2.5 py-1.5 rounded-lg border flex-shrink-0 transition-colors',
                isMapped
                  ? 'bg-slate-800 border-slate-700 text-indigo-400 hover:bg-slate-700'
                  : hasData
                  ? 'text-indigo-300 border-indigo-700 hover:bg-indigo-900/30'
                  : 'bg-slate-800 border-slate-700 text-slate-500 hover:bg-slate-700',
              )}
              style={hasData && !isMapped ? { background: 'rgba(49,46,129,.4)' } : undefined}
            >
              {isMapped ? '修改映射' : '配置映射 →'}
            </button>
          </div>
        )
      })}
    </div>
  )
}

// ── Sync history panel ─────────────────────────────────────────────────────
function SyncHistoryPanel({ srcId }: { srcId: string }) {
  const [jobs, setJobs] = useState<SyncJob[]>([])

  useEffect(() => {
    sourcesApi.jobs(srcId).then(setJobs).catch(console.error)
  }, [srcId])

  if (!jobs.length) return <p className="text-xs text-slate-500">暂无记录</p>

  const statusColor: Record<string, string> = {
    completed: 'text-emerald-400', failed: 'text-red-400', running: 'text-yellow-400', pending: 'text-slate-400',
  }

  return (
    <div className="space-y-1.5">
      {jobs.slice(0, 10).map((j, i) => (
        <div key={j.id ?? i} className="flex items-center justify-between px-3 py-2 rounded-lg border border-slate-800">
          <span className={cn('text-xs', statusColor[j.status] ?? 'text-slate-400')}>{j.status}</span>
          {j.total_records != null && <span className="text-xs text-slate-500">{j.total_records} 条</span>}
          {j.error && <span className="text-xs text-red-400 truncate ml-2 max-w-40">{j.error}</span>}
        </div>
      ))}
    </div>
  )
}

// ── Test connection panel ──────────────────────────────────────────────────
function TestPanel({ srcType, getConfig, selectedFiles, onFilesChange }: {
  srcType: string
  getConfig: () => Record<string, unknown>
  selectedFiles: Set<string>
  onFilesChange: (files: Set<string>) => void
}) {
  const [result,  setResult]  = useState<{ ok: boolean; message?: string; files?: string[] } | null>(null)
  const [testing, setTesting] = useState(false)

  function setSelectedFiles(updater: Set<string> | ((prev: Set<string>) => Set<string>)) {
    onFilesChange(typeof updater === 'function' ? updater(selectedFiles) : updater)
  }

  async function handleTest() {
    setTesting(true)
    setResult(null)
    try {
      const cfg = getConfig()
      const d   = await sourcesApi.quickTest({ source_type: srcType, config: cfg })
      if (d.status === 'error') {
        setResult({ ok: false, message: d.error || '连接失败' })
      } else {
        setResult({ ok: true, message: '连接成功', files: d.files })
      }
    } catch (e) {
      setResult({ ok: false, message: String(e) })
    } finally {
      setTesting(false)
    }
  }

  function toggleFile(f: string) {
    setSelectedFiles(prev => {
      const next = new Set(prev)
      if (next.has(f)) next.delete(f); else next.add(f)
      return next
    })
  }

  return (
    <div className="space-y-3">
      <button onClick={handleTest} disabled={testing}
        className="text-xs px-3 py-2 border border-slate-700 rounded-lg text-slate-300 hover:bg-slate-800 transition-colors disabled:opacity-50">
        🔌 {testing ? '测试中…' : '测试连接 & 发现文件'}
      </button>

      {result && (
        <div>
          <p className={cn('text-xs mb-2', result.ok ? 'text-emerald-400' : 'text-red-400')}>
            {result.ok ? '✓' : '✗'} {result.message}
          </p>
          {result.ok && (!result.files || result.files.length === 0) && (
            <p className="text-xs text-slate-500">未发现文件 — 检查 Bucket 和路径前缀</p>
          )}
          {result.files && result.files.length > 0 && (
            <div>
              {selectedFiles.size > 0 && (
                <p className="text-[11px] text-amber-400/80 mb-2">已选 {selectedFiles.size} 个文件 — 点击左侧「保存」后再同步</p>
              )}
              <div className="flex items-center justify-between mb-2">
                <p className="text-xs text-slate-500">发现 {result.files.length} 个文件（已选 {selectedFiles.size} 个）</p>
                <div className="flex gap-2">
                  <button onClick={() => setSelectedFiles(new Set(result.files!))}
                    className="text-[11px] px-2 py-0.5 border border-slate-700 rounded text-slate-400 hover:bg-slate-800">全选</button>
                  <button onClick={() => setSelectedFiles(new Set())}
                    className="text-[11px] px-2 py-0.5 border border-slate-700 rounded text-slate-400 hover:bg-slate-800">取消</button>
                </div>
              </div>
              <div className="border border-slate-800 rounded-lg max-h-48 overflow-y-auto">
                {result.files.map(f => {
                  const sel   = selectedFiles.has(f)
                  const fname = f.split('/').pop() ?? f
                  const fdir  = f.includes('/') ? f.substring(0, f.lastIndexOf('/') + 1) : ''
                  return (
                    <label key={f} className={cn('flex items-center gap-3 px-3 py-2 cursor-pointer border-b border-slate-800/60 last:border-0', sel && 'bg-indigo-900/20')}>
                      <input type="checkbox" checked={sel} onChange={() => toggleFile(f)} className="accent-indigo-500 flex-shrink-0" />
                      <span className="font-mono text-xs truncate">
                        <span className="text-slate-600">{fdir}</span>
                        <span className={sel ? 'text-indigo-300' : 'text-slate-300'}>{fname}</span>
                      </span>
                    </label>
                  )
                })}
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  )
}

// ── Source detail panel ────────────────────────────────────────────────────
type SrcTab = 'test' | 'history' | 'datasets'
interface SourcePanelProps {
  src: DataSource | null        // null = new source
  projectId: string
  onSaved: (src: DataSource) => void
  onDeleted: () => void
  onCancel: () => void
}

function SourcePanel({ src, projectId, onSaved, onDeleted, onCancel }: SourcePanelProps) {
  const formRef     = useRef<HTMLFormElement>(null)
  const [tab,       setTab]       = useState<SrcTab>('test')
  const [srcType,   setSrcType]   = useState(src?.source_type ?? 's3')
  const [selFiles, setSelFiles] = useState<Set<string>>(new Set(((src?.config as S3Config)?.selected_files) ?? []))
  const [saving,    setSaving]    = useState(false)
  const [syncing,   setSyncing]   = useState(false)
  const isNew = !src

  function readFormConfig(): Record<string, unknown> {
    if (!formRef.current) return {}
    const fd = new FormData(formRef.current)
    const v  = (n: string) => (fd.get(n) as string | null)?.trim() ?? ''
    if (srcType === 's3') {
      return {
        provider:       v('_provider')   || v('provider')   || 'rustfs',
        endpoint:       v('_endpoint')   || v('endpoint')   || '',
        bucket:         v('_bucket')     || v('bucket')     || '',
        prefix:         v('_prefix')     || v('prefix')     || '',
        access_key:     v('_access_key') || v('access_key') || '',
        secret_key:     v('_secret_key') || v('secret_key') || '',
        pem_cert:       v('_pem_cert')   || v('pem_cert')   || '',
        selected_files: [...selFiles],
      }
    }
    if (srcType === 'csv')  return { file_path: v('file_path') }
    if (srcType === 'db')   return { dsn: v('dsn') }
    if (srcType === 'rest') return { url: v('url'), method: v('method') || 'GET' }
    return {}
  }

  async function handleSave() {
    if (!formRef.current) return
    const name = (formRef.current.elements.namedItem('name') as HTMLInputElement)?.value?.trim()
    if (!name) { toast.error('名称不能为空'); return }
    setSaving(true)
    try {
      const body = { name, source_type: srcType, config: readFormConfig() }
      const saved = src
        ? await sourcesApi.update(src.id, body)
        : await sourcesApi.create(body)
      toast.success('已保存')
      onSaved(saved)
    } catch (e) {
      toast.error(String(e))
    } finally {
      setSaving(false)
    }
  }

  async function handleSync() {
    if (!src) return
    setSyncing(true)
    try {
      await sourcesApi.sync(src.id)
      toast.success('同步已启动')
      // Poll until job completes, then refresh source
      const poll = setInterval(async () => {
        try {
          const jobs = await sourcesApi.jobs(src.id)
          const latest = jobs[0]
          if (latest && (latest.status === 'completed' || latest.status === 'failed')) {
            clearInterval(poll)
            setSyncing(false)
            // Refetch source to get updated last_sync_at
            const srcs = await sourcesApi.list()
            const updated = srcs.find(s => s.id === src.id)
            if (updated) onSaved(updated)
          }
        } catch { clearInterval(poll); setSyncing(false) }
      }, 2000)
    } catch (e) {
      toast.error(String(e))
      setSyncing(false)
    }
  }

  async function handleToggle() {
    if (!src) return
    try {
      if (src.deprecated) await sourcesApi.activate(src.id)
      else await sourcesApi.deprecate(src.id)
      toast.success(src.deprecated ? '已启用' : '已停用')
      onSaved({ ...src, deprecated: !src.deprecated })
    } catch (e) {
      toast.error(String(e))
    }
  }

  async function handleDelete() {
    if (!src || !confirm(`确认删除「${src.name}」？`)) return
    try {
      await sourcesApi.delete(src.id)
      toast.success('已删除')
      onDeleted()
    } catch (e) {
      toast.error(String(e))
    }
  }

  const cfg = (src?.config ?? {}) as Record<string, unknown>

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Header */}
      {!isNew && (
        <div className="flex items-center justify-between px-6 py-3 border-b border-slate-800 flex-shrink-0">
          <h3 className="text-sm font-semibold text-slate-200">{src?.name}</h3>
          <span className="text-xs text-slate-500">
            {src?.last_sync_at ? `上次同步：${new Date(Number(src.last_sync_at) * 1000).toLocaleString('zh-CN')}` : '从未同步'}
          </span>
        </div>
      )}

      <div className="flex flex-1 overflow-hidden">
        {/* Left: form + action bar */}
        <div className="flex flex-col border-r border-slate-800 overflow-hidden" style={{ flex: '1.4' }}>
          <form ref={formRef} className="flex-1 overflow-y-auto p-6">
            <div className="grid grid-cols-2 gap-4">
              <div>
                <label className="text-xs text-slate-400 block mb-1">数据源名称 *</label>
                <input name="name" defaultValue={src?.name ?? ''} placeholder="如：HR 员工数据"
                  className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500" />
              </div>
              <div>
                <label className="text-xs text-slate-400 block mb-1">类型</label>
                <select name="source_type" value={srcType} onChange={e => setSrcType(e.target.value)}
                  className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500">
                  {Object.entries(SRC_LABEL).map(([v, l]) => <option key={v} value={v}>{l}</option>)}
                </select>
              </div>

              {/* Type-specific fields */}
              {srcType === 's3' && (
                <S3Form cfg={cfg as S3Config} selectedFiles={selFiles} />
              )}
              {srcType === 'csv' && (
                <div className="col-span-2">
                  <label className="text-xs text-slate-400 block mb-1">文件路径或 URL</label>
                  <input name="file_path" defaultValue={(cfg.file_path as string) ?? ''} placeholder="/data/employees.csv 或 http://…"
                    className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500" />
                </div>
              )}
              {srcType === 'db' && (
                <div className="col-span-2">
                  <label className="text-xs text-slate-400 block mb-1">连接字符串 (DSN)</label>
                  <input name="dsn" defaultValue={(cfg.dsn as string) ?? ''} placeholder="postgres://user:pass@host/db"
                    className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500" />
                </div>
              )}
              {srcType === 'rest' && (
                <>
                  <div className="col-span-2">
                    <label className="text-xs text-slate-400 block mb-1">API URL</label>
                    <input name="url" defaultValue={(cfg.url as string) ?? ''} placeholder="https://api.example.com/data"
                      className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500" />
                  </div>
                  <div>
                    <label className="text-xs text-slate-400 block mb-1">HTTP Method</label>
                    <select name="method" defaultValue={(cfg.method as string) ?? 'GET'}
                      className="w-full bg-slate-950 border border-slate-700 rounded-lg px-3 py-2 text-sm text-slate-200 focus:outline-none focus:border-indigo-500">
                      <option value="GET">GET</option>
                      <option value="POST">POST</option>
                    </select>
                  </div>
                </>
              )}
            </div>
          </form>

          {/* Action bar */}
          <div className="px-6 py-3 border-t border-slate-800 flex gap-2 items-center flex-shrink-0">
            <button onClick={handleSave} disabled={saving}
              className="px-3 py-1.5 text-xs bg-indigo-600 hover:bg-indigo-500 disabled:opacity-50 text-white rounded-lg transition-colors">
              {saving ? '保存中…' : '保存'}
            </button>
            {!isNew && (
              <>
                <button onClick={handleSync} disabled={syncing || !!src?.deprecated}
                  className="px-3 py-1.5 text-xs bg-emerald-700 hover:bg-emerald-600 disabled:opacity-50 text-white rounded-lg transition-colors">
                  {syncing ? '同步中…' : '▶ 同步'}
                </button>
                <button onClick={handleToggle}
                  className={cn('px-3 py-1.5 text-xs border rounded-lg transition-colors',
                    src?.deprecated
                      ? 'bg-emerald-700 hover:bg-emerald-600 text-white border-emerald-600'
                      : 'border-slate-700 text-slate-400 hover:bg-slate-800')}>
                  {src?.deprecated ? '启用' : '停用'}
                </button>
                <button onClick={handleDelete}
                  className="px-3 py-1.5 text-xs bg-red-900/40 hover:bg-red-800/60 text-red-400 border border-red-800/40 rounded-lg transition-colors">
                  删除
                </button>
              </>
            )}
            <button onClick={onCancel} className="px-3 py-1.5 text-xs border border-slate-700 text-slate-400 hover:bg-slate-800 rounded-lg transition-colors">
              取消
            </button>
          </div>
        </div>

        {/* Right: tabs */}
        <div className="flex flex-col flex-1 overflow-hidden">
          <div className="flex border-b border-slate-800 flex-shrink-0 px-1">
            {([['test','测试'], ['history','同步历史'], ['datasets','Datasets']] as [SrcTab,string][]).map(([t, l]) => (
              <button key={t} onClick={() => setTab(t)}
                className={cn('px-4 py-2.5 text-xs border-b-2 transition-colors',
                  tab === t ? 'border-indigo-500 text-indigo-300' : 'border-transparent text-slate-500 hover:text-slate-300')}>
                {l}
              </button>
            ))}
          </div>
          <div className="flex-1 overflow-y-auto p-4">
            {tab === 'test'     && <TestPanel srcType={srcType} getConfig={readFormConfig} selectedFiles={selFiles} onFilesChange={setSelFiles} />}
            {tab === 'history'  && !isNew && <SyncHistoryPanel srcId={src!.id} />}
            {tab === 'datasets' && !isNew && <DatasetsPanel srcId={src!.id} projectId={projectId} />}
            {(tab === 'history' || tab === 'datasets') && isNew && (
              <p className="text-xs text-slate-600">先保存数据源</p>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

// ── WorkspacePage ──────────────────────────────────────────────────────────
const WS_TABS = [
  { key: 'ingest',   label: '数据接入' },
  { key: 'import',   label: '导入' },
  { key: 'schema',   label: '模型' },
  { key: 'browse',   label: '浏览' },
  { key: 'graph',    label: '图谱' },
] as const
type WsTab = typeof WS_TABS[number]['key']

export default function WorkspacePage() {
  const { projectId } = useParams<{ projectId: string }>()
  const [searchParams, setSearchParams] = useSearchParams()
  const wsTab = (searchParams.get('tab') ?? 'ingest') as WsTab

  const [project,    setProject]    = useState<Project | null>(null)
  const [sources,    setSources]    = useState<DataSource[]>([])
  const [currentSrc, setCurrentSrc] = useState<DataSource | null>(null)
  const [showNew,    setShowNew]    = useState(false)

  useEffect(() => {
    if (!projectId) return
    projectsApi.get(projectId).then(setProject).catch(console.error)
    loadSources()
  }, [projectId])

  async function loadSources() {
    try {
      const srcs = await sourcesApi.list()
      setSources(srcs)
      // Auto-select first if none selected
      if (!currentSrc && srcs.length) setCurrentSrc(srcs[0])
    } catch (e) {
      console.error(e)
    }
  }

  function handleSrcSaved(saved: DataSource) {
    setSources(prev => {
      const idx = prev.findIndex(s => s.id === saved.id)
      if (idx >= 0) { const next = [...prev]; next[idx] = saved; return next }
      return [...prev, saved]
    })
    setCurrentSrc(saved)
    setShowNew(false)
  }

  function handleSrcDeleted() {
    setSources(prev => prev.filter(s => s.id !== currentSrc?.id))
    setCurrentSrc(null)
    setShowNew(false)
  }

  function handleCancel() {
    if (showNew) { setShowNew(false); return }
    // just restore
  }

  if (!project) return <div className="p-8 text-slate-500 text-sm">加载中…</div>

  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Tab bar */}
      <div className="flex items-center gap-0 px-4 border-b border-slate-800 flex-shrink-0 h-11">
        {WS_TABS.map(t => (
          <button key={t.key} onClick={() => setSearchParams({ tab: t.key })}
            className={cn('px-4 h-full text-sm border-b-2 transition-colors',
              wsTab === t.key
                ? 'border-indigo-500 text-indigo-300'
                : 'border-transparent text-slate-500 hover:text-slate-300')}>
            {t.label}
          </button>
        ))}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-hidden">
        {wsTab === 'import' && <ImportTab />}
        {wsTab === 'schema' && <SchemaTab projectId={projectId} />}
        {wsTab === 'browse' && <BrowseTab projectId={projectId ?? ''} />}
        {wsTab === 'graph'    && <GraphTab projectId={projectId} />}
        {wsTab === 'ingest' && (
          <div className="flex h-full overflow-hidden w-full">
            {/* Sidebar: source list */}
            <div className="w-52 flex-shrink-0 border-r border-slate-800 flex flex-col overflow-hidden">
              <div className="flex items-center justify-between px-3 py-2.5 border-b border-slate-800 flex-shrink-0">
                <p className="text-[11px] font-semibold text-slate-400 uppercase tracking-wider">数据源</p>
                <button onClick={() => { setShowNew(true); setCurrentSrc(null) }}
                  className="text-xs px-2.5 py-1 bg-indigo-600 hover:bg-indigo-500 text-white rounded-lg transition-colors">
                  + 新建
                </button>
              </div>
              <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
                {!sources.length && (
                  <p className="text-xs text-slate-600 text-center py-6">暂无数据源</p>
                )}
                {sources.map(s => {
                  const isActive = !showNew && currentSrc?.id === s.id
                  return (
                    <button key={s.id} onClick={() => { setCurrentSrc(s); setShowNew(false) }}
                      className={cn('w-full text-left px-2.5 py-2 rounded-lg flex items-center gap-2 transition-colors',
                        isActive ? 'bg-indigo-900/40 border border-indigo-700/40' : 'hover:bg-slate-800/60')}>
                      <span className="text-base flex-shrink-0">{SRC_ICON[s.source_type] || '📦'}</span>
                      <span className="text-sm text-slate-200 truncate flex-1">{s.name}</span>
                      <span className={cn('w-2 h-2 rounded-full flex-shrink-0', STATUS_DOT[s.status] || STATUS_DOT.idle)} />
                    </button>
                  )
                })}
              </div>
            </div>

            {/* Main: source detail or empty state */}
            <div className="flex-1 overflow-hidden">
              {!currentSrc && !showNew ? (
                <div className="flex flex-col items-center justify-center h-full gap-3 text-slate-600">
                  <div className="text-5xl opacity-30">🔌</div>
                  <p className="text-sm">选择一个数据源，或新建连接</p>
                </div>
              ) : (
                <SourcePanel
                  key={showNew ? 'new' : currentSrc?.id}
                  src={showNew ? null : currentSrc}
                  projectId={projectId!}
                  onSaved={handleSrcSaved}
                  onDeleted={handleSrcDeleted}
                  onCancel={handleCancel}
                />
              )}
            </div>
          </div>
        )}
      </div>
    </div>
  )
}

