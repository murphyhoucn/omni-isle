import { AnimatePresence, motion } from 'framer-motion'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useEffect, useMemo, useRef, useState } from 'react'
import './App.css'

const EXPANDED_WIDTH = 650
const COLLAPSED_WIDTH = 420
const APP_VERSION = '0.1.0-demo'
const APP_AUTHOR = 'OmniIsle Team'
const GITHUB_REPO_URL = 'https://github.com/'

type ViewMode = 'island' | 'settings'
type RunState = 'ready' | 'queued' | 'running' | 'success' | 'error'
type LogLevel = 'info' | 'warn' | 'queue' | 'stdout' | 'stderr' | 'success' | 'error'

const stateMeta: Record<RunState, { text: string; tone: string }> = {
  ready: { text: '准备就绪', tone: 'tone-ready' },
  queued: { text: '排队中', tone: 'tone-queued' },
  running: { text: '处理中...', tone: 'tone-running' },
  success: { text: '执行成功', tone: 'tone-success' },
  error: { text: '执行失败', tone: 'tone-error' },
}

type LogEntry = {
  id: number
  level: LogLevel
  text: string
  at: string
}

type ScriptRunResult = {
  success: boolean
  exit_code: number
  stdout: string
  stderr: string
}

type ScriptLogEvent = {
  stream: 'stdout' | 'stderr'
  line: string
}

type ScriptMenuItem = {
  label: string
  script: string
}

type StartupJob = {
  script: string
  target_path?: string | null
}

type QueueJob = {
  label: string
  script: string
  targetPath?: string
}

const nowStamp = () =>
  new Date().toLocaleTimeString('zh-CN', {
    hour12: false,
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })

function App() {
  const [viewMode, setViewMode] = useState<ViewMode>('island')
  const [wide, setWide] = useState(false)
  const [showPanel, setShowPanel] = useState(false)
  const [runState, setRunState] = useState<RunState>('ready')
  const [logs, setLogs] = useState<LogEntry[]>([
    { id: 1, level: 'info', text: 'OmniIsle island online', at: nowStamp() },
  ])
  const [scriptItems, setScriptItems] = useState<ScriptMenuItem[]>([
    { label: '运行成功脚本', script: 'mock_success.py' },
    { label: '运行失败脚本(演示)', script: 'mock_error.py' },
  ])
  const [busy, setBusy] = useState(false)
  const [queue, setQueue] = useState<QueueJob[]>([])
  const [activeJob, setActiveJob] = useState<QueueJob | null>(null)

  const panelRef = useRef<HTMLDivElement | null>(null)
  const logRef = useRef<HTMLDivElement | null>(null)
  const logIdRef = useRef(2)
  const openTimer = useRef<number | null>(null)
  const closeTimer = useRef<number | null>(null)

  const meta = useMemo(() => stateMeta[runState], [runState])
  const statusText = useMemo(() => {
    if (runState === 'running' && queue.length > 0) {
      return `处理中... 队列 ${queue.length}`
    }
    if (runState === 'queued') {
      return queue.length > 0 ? `排队中 ${queue.length}` : '排队中'
    }
    return meta.text
  }, [meta.text, queue.length, runState])

  const clearTimers = () => {
    if (openTimer.current) {
      window.clearTimeout(openTimer.current)
      openTimer.current = null
    }
    if (closeTimer.current) {
      window.clearTimeout(closeTimer.current)
      closeTimer.current = null
    }
  }

  const openIsland = () => {
    clearTimers()
    setWide(true)
    openTimer.current = window.setTimeout(() => {
      setShowPanel(true)
      openTimer.current = null
    }, 85)
  }

  const closeIsland = () => {
    clearTimers()
    setShowPanel(false)
    setViewMode('island')
    closeTimer.current = window.setTimeout(() => {
      setWide(false)
      closeTimer.current = null
    }, 95)
  }

  useEffect(() => {
    let unlistenBlur: () => void;

    const setupBlurListener = async () => {
      unlistenBlur = await getCurrentWindow().listen('tauri://blur', () => {
        closeIsland()
      })
    }

    setupBlurListener()

    return () => {
      if (unlistenBlur) {
        unlistenBlur()
      }
    }
  }, [showPanel, wide, viewMode])

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      if (!panelRef.current) {
        return
      }
      if (!panelRef.current.contains(event.target as Node)) {
        closeIsland()
      }
    }

    if (showPanel || wide || viewMode === 'settings') {
      window.addEventListener('pointerdown', onPointerDown)
    }

    return () => {
      window.removeEventListener('pointerdown', onPointerDown)
    }
  }, [showPanel, wide, viewMode])

  useEffect(() => () => clearTimers(), [])

  useEffect(() => {
    if (!logRef.current) {
      return
    }
    logRef.current.scrollTop = logRef.current.scrollHeight
  }, [logs])

  useEffect(() => {
    if (viewMode === 'settings') {
      void invoke('sync_main_window_size', { mode: 'settings' })
      return
    }
    if (wide) {
      void invoke('sync_main_window_size', { mode: 'expanded' })
    } else {
      const timer = setTimeout(() => {
        void invoke('sync_main_window_size', { mode: 'collapsed' })
      }, 400) // Wait for shrink animation to finish before shrinking OS window
      return () => clearTimeout(timer)
    }
  }, [viewMode, wide])

  const pushLog = (level: LogLevel, text: string) => {
    const id = logIdRef.current
    logIdRef.current += 1

    setLogs((prev) => [...prev, { id, level, text, at: nowStamp() }].slice(-180))
  }

  useEffect(() => {
    const queueStartupJob = async (catalog: ScriptMenuItem[]) => {
      try {
        const startup = await invoke<StartupJob | null>('take_startup_job')
        if (!startup || !startup.script) {
          return
        }

        const matched = catalog.find((item) => item.script === startup.script)
        const targetPath = startup.target_path ?? undefined
        const nextJob: QueueJob = {
          label: matched?.label ?? startup.script,
          script: startup.script,
          targetPath,
        }

        setViewMode('island')
        openIsland()
        setQueue((prev) => [...prev, nextJob])
        setRunState((prev) => (prev === 'running' ? prev : 'queued'))
        pushLog('queue', `右键触发入队: ${nextJob.label}${targetPath ? ` (${targetPath})` : ''}`)
      } catch {
        pushLog('warn', '未能读取启动参数任务，继续待机')
      }
    }

    const loadScriptCatalog = async () => {
      let catalog = scriptItems

      try {
        const items = await invoke<ScriptMenuItem[]>('get_script_catalog')
        if (Array.isArray(items) && items.length > 0) {
          setScriptItems(items)
          catalog = items
        } else {
          pushLog('warn', '脚本配置为空，使用默认演示脚本')
        }
      } catch (error) {
        pushLog('warn', `读取脚本配置失败，使用默认演示脚本: ${String(error)}`)
      }

      await queueStartupJob(catalog)
    }

    void loadScriptCatalog()
  }, [])

  const executeJob = async (job: QueueJob) => {
    setBusy(true)
    setRunState('running')
    pushLog(
      'info',
      `开始执行: ${job.label} (${job.script})${job.targetPath ? ` -> ${job.targetPath}` : ''}`,
    )

    let gotStreamLog = false
    let unlisten: null | (() => void) = null

    try {
      try {
        unlisten = await listen<ScriptLogEvent>('script-log', (event) => {
          gotStreamLog = true
          if (event.payload.stream === 'stderr') {
            pushLog('stderr', event.payload.line)
          } else {
            pushLog('stdout', event.payload.line)
          }
        })
      } catch {
        pushLog('warn', '实时日志监听不可用，将在任务结束后显示输出')
      }

      const result = await invoke<ScriptRunResult>('run_demo_script', {
        scriptName: job.script,
        targetPath: job.targetPath,
      })

      if (!gotStreamLog) {
        const stdoutLines = result.stdout
          .split(/\r?\n/)
          .map((line) => line.trim())
          .filter(Boolean)
        const stderrLines = result.stderr
          .split(/\r?\n/)
          .map((line) => line.trim())
          .filter(Boolean)

        stdoutLines.forEach((line) => pushLog('stdout', line))
        stderrLines.forEach((line) => pushLog('stderr', line))
      }

      if (result.success) {
        setRunState('success')
        pushLog('success', `任务完成，退出码 ${result.exit_code}`)
      } else {
        setRunState('error')
        pushLog('error', `任务失败，退出码 ${result.exit_code}`)
      }
    } catch (error) {
      setRunState('error')
      pushLog('error', `Tauri 后端执行失败: ${String(error)}`)
    } finally {
      if (unlisten) {
        unlisten()
      }
      setBusy(false)
      setActiveJob(null)
    }
  }

  useEffect(() => {
    if (busy || activeJob || queue.length === 0) {
      return
    }

    const [next, ...rest] = queue
    setQueue(rest)
    setActiveJob(next)
    void executeJob(next)
  }, [activeJob, busy, queue])

  const panelVisible = showPanel || viewMode === 'settings'
  const shellWidth = wide || viewMode === 'settings' ? EXPANDED_WIDTH : COLLAPSED_WIDTH

  return (
    <main className="stage">
      <motion.section
        className="island-wrap"
        initial={{ opacity: 0, y: -24 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.45, ease: [0.22, 1, 0.36, 1] }}
      >
        <motion.div
          ref={panelRef}
          className={`island-shell ${meta.tone}`}
          animate={{ width: shellWidth }}
          transition={{ type: 'spring', stiffness: 440, damping: 34, mass: 0.62 }}
        >
          <button
            className="island-pill"
            onClick={() => {
              if (viewMode === 'settings') {
                setViewMode('island')
                closeIsland()
                return
              }

              if (showPanel || wide) {
                closeIsland()
              } else {
                openIsland()
              }
            }}
            type="button"
          >
            <span className="dot" />
              <span className="status-text">{viewMode === 'settings' ? '设置' : statusText}</span>              <span className="ghost">OmniIsle</span>
            </button>
          <AnimatePresence>
            {panelVisible && (
              <motion.div
                layout
                className="island-panel"
                initial={{ opacity: 0, height: 0, scale: 0.95 }}
                animate={{ opacity: 1, height: 'auto', scale: 1 }}
                exit={{ opacity: 0, height: 0, scale: 0.95 }}
                transition={{ duration: 0.2 }}
                style={{ overflow: 'hidden' }}
              >
                <div style={{ padding: '14px', position: 'relative' }}>
                  <AnimatePresence mode="popLayout" initial={false}>
                    {viewMode === 'settings' ? (
                      <motion.section
                        layout
                        key="panel-settings"
                        className="panel-settings-view"
                      initial={{ opacity: 0 }}
                      animate={{ opacity: 1 }}
                      exit={{ opacity: 0 }}
                      transition={{ duration: 0.18 }}
                    >
                      <header className="settings-top panel-settings-top">
                        <button
                          type="button"
                          className="back-btn"
                          onClick={() => setViewMode('island')}
                          aria-label="返回"
                        >
                          <svg viewBox="0 0 24 24" aria-hidden="true">
                            <path
                              fill="currentColor"
                              d="M14.71 6.29a1 1 0 0 1 0 1.42L10.41 12l4.3 4.29a1 1 0 1 1-1.42 1.42l-5-5a1 1 0 0 1 0-1.42l5-5a1 1 0 0 1 1.42 0Z"
                            />
                          </svg>
                        </button>
                        <h1>OmniIsle 设置</h1>
                      </header>

                      <section className="settings-card panel-settings-card">
                        <h2>开机自启动</h2>
                        {/* <p>启动系统后自动拉起 OmniIsle。</p> */}
                        <div className="setting-row">
                          <button type="button" className="switch-btn" aria-pressed="false">
                            <span className="switch-thumb" />
                          </button>
                          <span className="setting-tip">预览态，后续接入系统权限与真实开关</span>
                        </div>
                      </section>

                      <section className="settings-card panel-settings-card">
                        <h2>脚本管理</h2>
                        <p>维护执行脚本清单，后续支持添加、编辑与排序。</p>
                        <ul className="script-list">
                          {scriptItems.map((item) => (
                            <li key={`manage-${item.script}`}>
                              <div className="script-label" title={`${item.label}\n${item.script}`}>
                                <span className="script-name">{item.label}</span>
                                <span className="script-file">{item.script}</span>
                              </div>
                              <div className="script-actions">
                                <button type="button" className="script-icon-btn" disabled aria-label="编辑脚本">
                                  <svg viewBox="0 0 24 24" aria-hidden="true">
                                    <path
                                      fill="currentColor"
                                      d="M3 17.25V21h3.75L17.8 9.94l-3.75-3.75L3 17.25Zm17.71-10.04a1.003 1.003 0 0 0 0-1.42l-2.5-2.5a1.003 1.003 0 0 0-1.42 0l-1.96 1.96 3.75 3.75 2.13-1.79Z"
                                    />
                                  </svg>
                                </button>
                                <button type="button" className="script-icon-btn" disabled aria-label="删除脚本">
                                  <svg viewBox="0 0 24 24" aria-hidden="true">
                                    <path
                                      fill="currentColor"
                                      d="M6 7h12l-1 14H7L6 7Zm3-3h6l1 2h4v2H4V6h4l1-2Z"
                                    />
                                  </svg>
                                </button>
                              </div>
                            </li>
                          ))}
                        </ul>
                        <button type="button" className="ghost-btn" disabled>
                          + 添加脚本（后续开放）
                        </button>
                      </section>

                      <section className="settings-card panel-settings-card">
                        <h2>版本信息</h2>
                        <div className="about-lines">
                          <p>
                            <span>产品</span>
                            <strong>OmniIsle</strong>
                            <span className="sep">|</span>
                            <span>版本</span>
                            <strong>{APP_VERSION}</strong>
                          </p>
                          <p>
                            <span>作者</span>
                            <strong>{APP_AUTHOR}</strong>
                            <a
                              className="github-link compact"
                              href={GITHUB_REPO_URL}
                              target="_blank"
                              rel="noreferrer noopener"
                              aria-label="打开 Github 仓库"
                            >
                              <svg viewBox="0 0 24 24" aria-hidden="true">
                                <path
                                  fill="currentColor"
                                  d="M12 2C6.48 2 2 6.58 2 12.22c0 4.5 2.87 8.32 6.84 9.66.5.1.68-.22.68-.49 0-.24-.01-1.03-.01-1.86-2.78.62-3.37-1.21-3.37-1.21-.45-1.2-1.11-1.52-1.11-1.52-.91-.64.07-.63.07-.63 1 .08 1.53 1.05 1.53 1.05.9 1.57 2.36 1.12 2.93.86.09-.67.35-1.12.63-1.38-2.22-.26-4.56-1.14-4.56-5.06 0-1.12.39-2.04 1.03-2.75-.1-.26-.45-1.31.1-2.72 0 0 .84-.27 2.75 1.05A9.28 9.28 0 0 1 12 6.77a9.3 9.3 0 0 1 2.5.35c1.9-1.32 2.74-1.05 2.74-1.05.55 1.41.2 2.46.1 2.72.64.71 1.03 1.63 1.03 2.75 0 3.93-2.34 4.8-4.57 5.05.36.31.67.92.67 1.86 0 1.34-.01 2.42-.01 2.74 0 .27.18.59.69.49A10.25 10.25 0 0 0 22 12.22C22 6.58 17.52 2 12 2Z"
                                />
                              </svg>
                            </a>
                          </p>
                        </div>
                      </section>
                    </motion.section>
                  ) : (
                    <motion.section
                      key="panel-runtime"
                      className="panel-runtime-view"
                      initial={{ opacity: 0 }}
                      animate={{ opacity: 1 }}
                      exit={{ opacity: 0 }}
                      transition={{ duration: 0.18 }}
                    >
                      <button
                        type="button"
                        className="settings-entry panel-settings-entry"
                        onClick={() => {
                          setViewMode('settings')
                          openIsland()
                        }}
                        aria-label="进入设置"
                      >
                        <svg viewBox="0 0 24 24" aria-hidden="true">
                          <path
                            fill="currentColor"
                            d="M19.14 12.94a7.9 7.9 0 0 0 .05-.94c0-.32-.02-.63-.05-.94l2.03-1.58a.5.5 0 0 0 .12-.64l-1.92-3.32a.5.5 0 0 0-.6-.22l-2.39.96a7.18 7.18 0 0 0-1.63-.94l-.36-2.54a.5.5 0 0 0-.5-.42h-3.84a.5.5 0 0 0-.5.42l-.36 2.54c-.58.23-1.12.55-1.63.94l-2.39-.96a.5.5 0 0 0-.6.22L2.68 8.84a.5.5 0 0 0 .12.64l2.03 1.58c-.03.31-.05.62-.05.94 0 .32.02.63.05.94L2.8 14.52a.5.5 0 0 0-.12.64l1.92 3.32a.5.5 0 0 0 .6.22l2.39-.96c.5.39 1.05.71 1.63.94l.36 2.54a.5.5 0 0 0 .5.42h3.84a.5.5 0 0 0 .5-.42l.36-2.54c.58-.23 1.12-.55 1.63-.94l2.39.96a.5.5 0 0 0 .6-.22l1.92-3.32a.5.5 0 0 0-.12-.64l-2.03-1.58ZM12 15.5a3.5 3.5 0 1 1 0-7 3.5 3.5 0 0 1 0 7Z"
                          />
                        </svg>
                      </button>

                      <header className="panel-head">
                        <h1>OmniIsle</h1>
                        <p>
                          {activeJob
                            ? `当前执行: ${activeJob.label}`
                            : queue.length > 0
                              ? `等待队列: ${queue.length} 项`
                              : '等待外部触发脚本任务...'}
                        </p>
                      </header>

                      <div className="log-box" role="log" aria-live="polite" ref={logRef}>
                        {logs.map((entry) => (
                          <p key={entry.id} className={`log-line log-${entry.level}`}>
                            <span className="log-time">{entry.at}</span>
                            <span className="log-tag">{entry.level}</span>
                            <span className="log-text">{entry.text}</span>
                          </p>
                        ))}
                      </div>
                    </motion.section>
                  )}
                </AnimatePresence>
                </div>
              </motion.div>
            )}
          </AnimatePresence>
        </motion.div>
      </motion.section>
    </main>
  )
}

export default App
