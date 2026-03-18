import { AnimatePresence, motion } from 'framer-motion'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useEffect, useMemo, useRef, useState } from 'react'
import './App.css'

const EXPANDED_WIDTH = 650
const COLLAPSED_WIDTH = 300
const APP_VERSION = '0.1.0'
const APP_AUTHOR = 'Murphy Hou'
const GITHUB_REPO_URL = 'https://github.com/murphyhoucn/omni-isle'
const DEBUG_BODY_BOUNDS = false // 调试用，是否显示主窗口边界框

const POST_RUN_HIDE_SECONDS = 5 // 任务完成后自动隐藏的短暂延迟（秒）

const IDLE_HIDE_OPTIONS = [
  { value: 5, label: '5 秒' },
  { value: 10, label: '10 秒' },
  { value: 30, label: '30 秒' },
  { value: 60, label: '60 秒' },
  { value: 120, label: '120 秒' },
  { value: 0, label: '永不自动隐藏' },
] as const

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

type SystemIntegrationConfig = {
  auto_start_enabled: boolean
  context_menu_enabled: boolean
}

type EnvConfigItem = {
  id: number
  name: string
  value: string
}

type EnvConfigPayload = {
  name: string
  executable_path: string
}

type DataDirectoryInfo = {
  data_root: string
  configs_dir: string
  logs_dir: string
  using_default: boolean
}

const nowStamp = () =>
  new Date().toLocaleTimeString('zh-CN', {
    hour12: false,
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })

/**
 * 主应用组件，包含整个应用的逻辑和UI渲染
 */
function App() {
  // 环境名称选项列表
  const envNameOptions = ['PYTHON', 'NODE', 'RUST', 'GCC/G++', 'JAVA', 'CUSTOM']



  // 视图模式状态管理
  const [viewMode, setViewMode] = useState<ViewMode>('island') // 当前视图模式
  const [wide, setWide] = useState(false) // 是否展开状态
  const [showPanel, setShowPanel] = useState(false)
  const [runState, setRunState] = useState<RunState>('ready')
  const [windowVisible, setWindowVisible] = useState(true)
  const [logs, setLogs] = useState<LogEntry[]>([ // 日志列表
    { id: 1, level: 'info', text: 'OmniIsle online', at: nowStamp() },
  ])
  const [scriptItems, setScriptItems] = useState<ScriptMenuItem[]>([]) // 脚本项目列表
  const [busy, setBusy] = useState(false) // 是否忙碌状态
  const [savingScripts, setSavingScripts] = useState(false) // 是否正在保存脚本
  const [refreshingScripts, setRefreshingScripts] = useState(false) // 是否正在刷新脚本列表
  const [queue, setQueue] = useState<QueueJob[]>([]) // 任务队列
  const [activeJob, setActiveJob] = useState<QueueJob | null>(null) // 当前活动任务
  const [autoStartEnabled, setAutoStartEnabled] = useState(false) // 是否启用自启动
  const [contextMenuEnabled, setContextMenuEnabled] = useState(false) // 是否启用右键菜单
  const [savingSystemIntegration, setSavingSystemIntegration] = useState(false) // 是否正在保存系统集成设置
  const [savingEnvConfigs, setSavingEnvConfigs] = useState(false) // 是否正在保存环境配置
  const [savingIdleHide, setSavingIdleHide] = useState(false) // 是否正在保存自动隐藏设置
  const [savingDataDirectory, setSavingDataDirectory] = useState(false) // 是否正在保存数据目录
  const [openingUserDataFolder, setOpeningUserDataFolder] = useState(false) // 是否正在打开用户数据目录
  const [pickingDataDirectory, setPickingDataDirectory] = useState(false) // 是否正在选择数据目录
  const [idleHideSeconds, setIdleHideSeconds] = useState(60) // 自动隐藏秒数，0=永不隐藏
  const [dataDirectoryInput, setDataDirectoryInput] = useState('') // 数据目录输入值
  const [pickingEnvRowId, setPickingEnvRowId] = useState<number | null>(null) // 正在选择的环境配置ID
  const [editingScriptName, setEditingScriptName] = useState<string | null>(null) // 正在编辑的脚本名称
  const [actionFeedbackKey, setActionFeedbackKey] = useState<string | null>(null) // 操作反馈键
  const [showAddScriptForm, setShowAddScriptForm] = useState(false) // 是否显示添加脚本表单
  const [pendingDeleteScript, setPendingDeleteScript] = useState<ScriptMenuItem | null>(null) // 待确认删除的脚本
  const [newScriptName, setNewScriptName] = useState('') // 新脚本名称
  const [newScriptLabel, setNewScriptLabel] = useState('') // 新脚本标签
  const [envConfigs, setEnvConfigs] = useState<EnvConfigItem[]>([ // 环境配置列表
    { id: 1, name: 'PYTHON', value: 'python' },
    { id: 2, name: 'NODE', value: 'node' },
  ])



  // DOM引用
  const panelRef = useRef<HTMLDivElement | null>(null) // 面板引用
  const logRef = useRef<HTMLDivElement | null>(null) // 日志区域引用
  const logIdRef = useRef(2) // 日志ID计数器
  const scriptItemsRef = useRef<ScriptMenuItem[]>([])
  const envRowIdRef = useRef(3) // 环境配置行ID计数器
  const openTimer = useRef<number | null>(null) // 打开定时器
  const closeTimer = useRef<number | null>(null) // 关闭定时器
  const actionFeedbackTimer = useRef<number | null>(null) // 操作反馈定时器
  const lastInteractionMsRef = useRef(Date.now()) // 最近一次交互时间戳
  const idleHideInProgressRef = useRef(false) // 防止并发触发隐藏
  const justFinishedRunRef = useRef(false) // 标记刚刚完成了一次任务运行



  // 计算属性
  const meta = useMemo(() => stateMeta[runState], [runState]) // 根据运行状态获取元数据
  const statusText = useMemo(() => { // 状态文本
    if (runState === 'running' && queue.length > 0) {
      return `处理中... 队列 ${queue.length}`
    }
    if (runState === 'queued') {
      return queue.length > 0 ? `排队中 ${queue.length}` : '排队中'
    }
    return meta.text
  }, [meta.text, queue.length, runState])

  // 清除所有定时器
  const clearTimers = () => {
    if (openTimer.current) {
      window.clearTimeout(openTimer.current)
      openTimer.current = null
    }
    if (closeTimer.current) {
      window.clearTimeout(closeTimer.current)
      closeTimer.current = null
    }
    if (actionFeedbackTimer.current) {
      window.clearTimeout(actionFeedbackTimer.current)
      actionFeedbackTimer.current = null
    }
  }

  const markInteraction = () => {
    lastInteractionMsRef.current = Date.now()
    justFinishedRunRef.current = false
  }

  const hideWindowByIdle = async () => {
    if (idleHideInProgressRef.current || !windowVisible) {
      return
    }
    idleHideInProgressRef.current = true
    logToFile('INFO', `idle auto-hide triggered: ${idleHideSeconds}s`)
    closeIsland()
    try {
      // Give the collapse animation a brief moment before hiding the native window.
      await new Promise((resolve) => {
        window.setTimeout(resolve, 120)
      })
      await getCurrentWindow().hide()
      setWindowVisible(false)
      pushLog('info', '长时间无交互，已自动隐藏界面（托盘常驻）')
    } catch (error) {
      logToFile('ERROR', `idle auto-hide hide() failed: ${String(error)}`)
    } finally {
      idleHideInProgressRef.current = false
    }
  }

  const saveIdleHideSeconds = async (nextSeconds: number) => {
    setSavingIdleHide(true)
    try {
      const saved = await invoke<number>('set_idle_hide_seconds', {
        idleHideSeconds: nextSeconds,
      })
      setIdleHideSeconds(saved)
      lastInteractionMsRef.current = Date.now()
      justFinishedRunRef.current = false
      pushLog('success', `自动隐藏已更新: ${saved === 0 ? '永不自动隐藏' : `${saved} 秒`}`)
    } catch (error) {
      const errMsg = `保存自动隐藏设置失败: ${String(error)}`
      pushLog('error', errMsg)
      logToFile('ERROR', errMsg)
    } finally {
      setSavingIdleHide(false)
    }
  }

  // 显示操作反馈
  const flashActionFeedback = (key: string) => {
    setActionFeedbackKey(key)
    if (actionFeedbackTimer.current) {
      window.clearTimeout(actionFeedbackTimer.current)
    }
    actionFeedbackTimer.current = window.setTimeout(() => {
      setActionFeedbackKey(null)
      actionFeedbackTimer.current = null
    }, 180)
  }

  // 打开面板
  const openIsland = () => {
    clearTimers()
    setWindowVisible(true)
    lastInteractionMsRef.current = Date.now()
    setWide(true)
    openTimer.current = window.setTimeout(() => {
      setShowPanel(true)
      openTimer.current = null
    }, 85)
  }

  // 关闭面板
  const closeIsland = () => {
    clearTimers()
    setShowPanel(false)
    setViewMode('island')
    closeTimer.current = window.setTimeout(() => {
      setWide(false)
      closeTimer.current = null
    }, 95)
  }

  // 监听窗口失焦事件
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
    let unlistenTrayShow: (() => void) | undefined

    const setupTrayShowListener = async () => {
      unlistenTrayShow = await listen('tray-show-island', async () => {
        try {
          await getCurrentWindow().show()
        } catch {
          // ignore show failures
        }
        setWindowVisible(true)
        setViewMode('island')
        openIsland()
        await consumePendingStartupJob('右键触发入队')
      })
    }

    void setupTrayShowListener()

    return () => {
      if (unlistenTrayShow) {
        unlistenTrayShow()
      }
    }
  }, [])

  useEffect(() => {
    if (!DEBUG_BODY_BOUNDS) {
      return
    }

    document.documentElement.classList.add('debug-body-bounds')
    document.body.classList.add('debug-body-bounds')

    return () => {
      document.documentElement.classList.remove('debug-body-bounds')
      document.body.classList.remove('debug-body-bounds')
    }
  }, [])

  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      markInteraction()
      if (!panelRef.current) {
        return
      }
      if (!panelRef.current.contains(event.target as Node)) {
        closeIsland()
      }
    }

    const onKeyDown = () => {
      markInteraction()
    }

    const onWheel = () => {
      markInteraction()
    }

    if (windowVisible) {
      window.addEventListener('pointerdown', onPointerDown)
      window.addEventListener('keydown', onKeyDown)
      window.addEventListener('wheel', onWheel)
    }

    return () => {
      window.removeEventListener('pointerdown', onPointerDown)
      window.removeEventListener('keydown', onKeyDown)
      window.removeEventListener('wheel', onWheel)
    }
  }, [windowVisible])

  useEffect(() => {
    if (!windowVisible || idleHideSeconds <= 0) {
      return
    }

    const check = window.setInterval(() => {
      if (busy || idleHideInProgressRef.current) {
        return
      }

      const limitSecs = justFinishedRunRef.current
        ? Math.min(POST_RUN_HIDE_SECONDS, idleHideSeconds)
        : idleHideSeconds
      const elapsedMs = Date.now() - lastInteractionMsRef.current
      if (elapsedMs >= limitSecs * 1000) {
        justFinishedRunRef.current = false
        void hideWindowByIdle()
      }
    }, 500)

    return () => {
      window.clearInterval(check)
    }
  }, [windowVisible, busy, idleHideSeconds])

  useEffect(() => () => {
    clearTimers()
  }, [])

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

  const enqueueStartupJob = (startup: StartupJob, source: string) => {
    if (!startup.script) {
      return
    }

    const matched = scriptItemsRef.current.find((item) => item.script === startup.script)
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
    pushLog('queue', `${source}: ${nextJob.label}${targetPath ? ` (${targetPath})` : ''}`)
    logToFile('INFO', `startup job enqueued: ${nextJob.script}${targetPath ? ` target=${targetPath}` : ''}`)
  }

  const consumePendingStartupJob = async (source: string) => {
    try {
      const startup = await invoke<StartupJob | null>('take_startup_job')
      if (!startup || !startup.script) {
        logToFile('INFO', `startup job not available for source: ${source}`)
        return false
      }

      enqueueStartupJob(startup, source)
      return true
    } catch {
      logToFile('WARN', `startup job consume failed for source: ${source}`)
      return false
    }
  }

  /** Write a message to the on-disk application log (fire-and-forget). */
  const logToFile = (level: 'ERROR' | 'WARN' | 'INFO', message: string) => {
    void invoke('write_app_log', { level, message }).catch(() => {})
  }

  useEffect(() => {
    scriptItemsRef.current = scriptItems
  }, [scriptItems])

  const queueStartupJob = async (catalog: ScriptMenuItem[]) => {
    try {
      scriptItemsRef.current = catalog
      const consumed = await consumePendingStartupJob('右键触发入队')
      if (!consumed) {
        return
      }
    } catch {
      pushLog('warn', '未能读取启动参数任务，继续待机')
    }
  }

  const reloadScriptCatalog = async (withStartupQueue = false) => {
    let catalog: ScriptMenuItem[] = []

    try {
      const items = await invoke<ScriptMenuItem[]>('get_script_catalog')
      if (Array.isArray(items)) {
        setScriptItems(items)
        catalog = items
      }
    } catch (error) {
      const errMsg = `读取脚本配置失败: ${String(error)}`
      pushLog('warn', errMsg)
      logToFile('WARN', errMsg)
    }

    if (withStartupQueue) {
      await queueStartupJob(catalog)
    }
  }

  const refreshScriptCatalog = async () => {
    setRefreshingScripts(true)
    try {
      await reloadScriptCatalog(false)
      pushLog('success', '脚本列表已刷新，右键菜单已同步更新')
    } finally {
      setRefreshingScripts(false)
    }
  }

  useEffect(() => {
    void reloadScriptCatalog(true)
  }, [])

  const resolveEnvExecutable = (envNames: string | string[], fallback: string) => {
    const candidates = Array.isArray(envNames) ? envNames : [envNames]
    const found = envConfigs.find((item) =>
      candidates.some((name) => item.name.trim().toUpperCase() === name.toUpperCase()),
    )
    const configured = found?.value.trim() ?? ''
    return configured || fallback
  }

  const runtimeHintForScript = (scriptName: string) => {
    const ext = scriptName.split('.').pop()?.toLowerCase() ?? ''

    if (ext === 'py') {
      return {
        runtime: 'Python',
        executable: resolveEnvExecutable('PYTHON', 'python'),
        action: `执行脚本: scripts/${scriptName}`,
      }
    }

    if (ext === 'js' || ext === 'mjs' || ext === 'cjs') {
      return {
        runtime: 'Node.js',
        executable: resolveEnvExecutable('NODE', 'node'),
        action: `执行脚本: scripts/${scriptName}`,
      }
    }

    if (ext === 'java') {
      return {
        runtime: 'Java (javac -> java)',
        executable: resolveEnvExecutable('JAVA', 'java'),
        action: `编译并运行: scripts/${scriptName}`,
      }
    }

    if (ext === 'c' || ext === 'cpp' || ext === 'cc' || ext === 'cxx') {
      return {
        runtime: 'GCC/G++',
        executable: resolveEnvExecutable(['GCC/G++', 'GCC'], 'gcc'),
        action: `编译并运行: scripts/${scriptName}`,
      }
    }

    return {
      runtime: '未知',
      executable: '-',
      action: `尝试执行: scripts/${scriptName}`,
    }
  }

  const executeJob = async (job: QueueJob) => {
    setBusy(true)
    setRunState('running')
    const runtimeHint = runtimeHintForScript(job.script)
    pushLog('info', `运行环境: ${runtimeHint.runtime} (${runtimeHint.executable})`)
    pushLog('info', runtimeHint.action)
    if (job.targetPath) {
      pushLog('info', `目标路径: ${job.targetPath}`)
    }

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
      const errMsg = `Tauri 后端执行失败: ${String(error)}`
      pushLog('error', errMsg)
      logToFile('ERROR', errMsg)
    } finally {
      if (unlisten) {
        unlisten()
      }
      lastInteractionMsRef.current = Date.now()
      justFinishedRunRef.current = true
      setBusy(false)
      setActiveJob(null)
    }
  }

  const queueScriptRun = (item: ScriptMenuItem) => {
    flashActionFeedback(`run-${item.script}`)

    const nextJob: QueueJob = {
      label: item.label,
      script: item.script,
    }

    setQueue((prev) => [...prev, nextJob])
    setRunState((prev) => (prev === 'running' ? prev : 'queued'))
    pushLog('queue', `设置页测试运行入队: ${item.label} (${item.script})`)
    setViewMode('island')
    openIsland()
  }

  const onEditScript = async (item: ScriptMenuItem) => {
    flashActionFeedback(`edit-${item.script}`)
    setEditingScriptName(item.script)
    try {
      await invoke('open_script_in_editor', { scriptName: item.script })
      pushLog('info', `已用默认编辑器打开: ${item.script}`)
    } catch (error) {
      const errMsg = `打开脚本失败: ${String(error)}`
      pushLog('error', errMsg)
      logToFile('ERROR', errMsg)
    } finally {
      setEditingScriptName((prev) => (prev === item.script ? null : prev))
    }
  }

  const requestDeleteScript = (item: ScriptMenuItem) => {
    setPendingDeleteScript(item)
  }

  const cancelDeleteScript = () => {
    if (savingScripts) {
      return
    }
    setPendingDeleteScript(null)
  }

  const confirmDeleteScript = async () => {
    if (!pendingDeleteScript) {
      return
    }

    const item = pendingDeleteScript
    setSavingScripts(true)
    try {
      const updated = await invoke<ScriptMenuItem[]>('delete_script_item', {
        scriptName: item.script,
      })
      setScriptItems(updated)
      pushLog('success', `已删除脚本: ${item.label} (${item.script})`)
      setPendingDeleteScript(null)
    } catch (error) {
      const errMsg = `删除脚本失败: ${String(error)}`
      pushLog('error', errMsg)
      logToFile('ERROR', errMsg)
    } finally {
      setSavingScripts(false)
    }
  }

  const startAddScript = () => {
    setShowAddScriptForm(true)
    setNewScriptName('')
    setNewScriptLabel('')
  }

  const cancelAddScript = () => {
    setShowAddScriptForm(false)
    setNewScriptName('')
    setNewScriptLabel('')
  }

  const onAddScript = async () => {
    const scriptName = newScriptName.trim()
    if (!scriptName) {
      pushLog('warn', '脚本文件名不能为空')
      return
    }

    const label = newScriptLabel.trim() || scriptName

    setSavingScripts(true)
    try {
      const updated = await invoke<ScriptMenuItem[]>('create_script_item', {
        label,
        scriptName,
      })
      setScriptItems(updated)
      pushLog('success', `已新增脚本并打开编辑器: ${scriptName}`)
      cancelAddScript()
    } catch (error) {
      const errMsg = `新增脚本失败: ${String(error)}`
      pushLog('error', errMsg)
      logToFile('ERROR', errMsg)
    } finally {
      setSavingScripts(false)
    }
  }

  const saveSystemIntegration = async (nextAutoStart: boolean, nextContextMenu: boolean) => {
    setSavingSystemIntegration(true)
    try {
      const result = await invoke<SystemIntegrationConfig>('set_system_integration_config', {
        autoStartEnabled: nextAutoStart,
        contextMenuEnabled: nextContextMenu,
      })

      setAutoStartEnabled(result.auto_start_enabled)
      setContextMenuEnabled(result.context_menu_enabled)
      pushLog(
        'success',
        `系统集成已更新: 开机自启动 ${result.auto_start_enabled ? '开启' : '关闭'}，右键菜单 ${result.context_menu_enabled ? '开启' : '关闭'}`,
      )
    } catch (error) {
      const errMsg = `系统集成设置失败: ${String(error)}`
      pushLog('error', errMsg)
      logToFile('ERROR', errMsg)
    } finally {
      setSavingSystemIntegration(false)
    }
  }

  const toEnvPayload = (rows: EnvConfigItem[]): EnvConfigPayload[] =>
    rows.map((row) => ({
      name: row.name,
      executable_path: row.value,
    }))

  const saveEnvConfigs = async (rows: EnvConfigItem[]) => {
    setSavingEnvConfigs(true)
    try {
      const saved = await invoke<EnvConfigPayload[]>('set_environment_configs', {
        environments: toEnvPayload(rows),
      })

      const normalized = saved.map((item, index) => ({
        id: index + 1,
        name: item.name,
        value: item.executable_path,
      }))
      setEnvConfigs(normalized)
      envRowIdRef.current = normalized.length + 1
      pushLog('success', '环境配置已写入用户主目录/.omniisle/app_configs.json')
    } catch (error) {
      const errMsg = `保存环境配置失败: ${String(error)}`
      pushLog('error', errMsg)
      logToFile('ERROR', errMsg)
    } finally {
      setSavingEnvConfigs(false)
    }
  }

  const addEnvConfigRow = async () => {
    const id = envRowIdRef.current
    envRowIdRef.current += 1
    const next = [...envConfigs, { id, name: 'CUSTOM', value: '' }]
    setEnvConfigs(next)
    await saveEnvConfigs(next)
  }

  const updateEnvConfigRow = async (id: number, patch: Partial<EnvConfigItem>) => {
    const next = envConfigs.map((item) => (item.id === id ? { ...item, ...patch } : item))
    setEnvConfigs(next)
    await saveEnvConfigs(next)
  }

  const removeEnvConfigRow = async (id: number) => {
    const next = envConfigs.filter((item) => item.id !== id)
    setEnvConfigs(next)
    await saveEnvConfigs(next)
  }

  const resolveEnvConfigRow = async (row: EnvConfigItem) => {
    setPickingEnvRowId(row.id)
    try {
      const picked = await invoke<string | null>('pick_environment_executable')
      if (!picked) {
        pushLog('warn', `${row.name} 环境选择已取消`)
        return
      }

      const next = envConfigs.map((item) =>
        item.id === row.id ? { ...item, value: picked } : item,
      )
      setEnvConfigs(next)
      await saveEnvConfigs(next)
      pushLog('info', `已选择 ${row.name} 运行路径: ${picked}`)
    } catch (error) {
      const errMsg = `打开环境选择器失败: ${String(error)}`
      pushLog('error', errMsg)
      logToFile('ERROR', errMsg)
    } finally {
      setPickingEnvRowId(null)
    }
  }

  const openUserDataFolder = async () => {
    setOpeningUserDataFolder(true)
    try {
      const folderPath = await invoke<string>('open_user_data_folder')
      pushLog('info', `已打开用户数据目录: ${folderPath}`)
    } catch (error) {
      const errMsg = `打开用户数据目录失败: ${String(error)}`
      pushLog('error', errMsg)
      logToFile('ERROR', errMsg)
    } finally {
      setOpeningUserDataFolder(false)
    }
  }

  const applyDataDirectoryInfo = (info: DataDirectoryInfo) => {
    setDataDirectoryInput(info.data_root)
  }

  const saveDataDirectory = async (nextRoot: string) => {
    setSavingDataDirectory(true)
    try {
      const saved = await invoke<DataDirectoryInfo>('set_data_directory_root', {
        dataRoot: nextRoot,
      })
      applyDataDirectoryInfo(saved)
      pushLog(
        'success',
        `数据目录已更新: ${saved.data_root}${saved.using_default ? '（默认）' : ''}`,
      )
    } catch (error) {
      const errMsg = `保存数据目录失败: ${String(error)}`
      pushLog('error', errMsg)
      logToFile('ERROR', errMsg)
    } finally {
      setSavingDataDirectory(false)
    }
  }

  const pickDataDirectory = async () => {
    setPickingDataDirectory(true)
    try {
      const picked = await invoke<string | null>('pick_data_directory')
      if (!picked) {
        return
      }
      await saveDataDirectory(picked)
    } catch (error) {
      const errMsg = `打开数据目录选择器失败: ${String(error)}`
      pushLog('error', errMsg)
      logToFile('ERROR', errMsg)
    } finally {
      setPickingDataDirectory(false)
    }
  }

  useEffect(() => {
    const loadSettings = async () => {
      try {
        const config = await invoke<SystemIntegrationConfig>('get_system_integration_config')
        setAutoStartEnabled(config.auto_start_enabled)
        setContextMenuEnabled(config.context_menu_enabled)
      } catch (error) {
        const errMsg = `读取系统集成配置失败: ${String(error)}`
        pushLog('warn', errMsg)
        logToFile('WARN', errMsg)
      }

      try {
        const environments = await invoke<EnvConfigPayload[]>('get_environment_configs')
        if (Array.isArray(environments) && environments.length > 0) {
          const mapped = environments.map((item, index) => ({
            id: index + 1,
            name: item.name,
            value: item.executable_path,
          }))
          setEnvConfigs(mapped)
          envRowIdRef.current = mapped.length + 1
        }
      } catch (error) {
        const errMsg = `读取环境配置失败: ${String(error)}`
        pushLog('warn', errMsg)
        logToFile('WARN', errMsg)
      }

      try {
        const seconds = await invoke<number>('get_idle_hide_seconds')
        setIdleHideSeconds(seconds)
      } catch (error) {
        const errMsg = `读取自动隐藏设置失败: ${String(error)}`
        pushLog('warn', errMsg)
        logToFile('WARN', errMsg)
      }

      try {
        const info = await invoke<DataDirectoryInfo>('get_data_directory_info')
        applyDataDirectoryInfo(info)
      } catch (error) {
        const errMsg = `读取数据目录设置失败: ${String(error)}`
        pushLog('warn', errMsg)
        logToFile('WARN', errMsg)
      }
    }

    void loadSettings()
  }, [])

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
            aria-label={viewMode === 'settings' ? '返回主界面' : showPanel || wide ? '收起面板' : '展开面板'}
            title={viewMode === 'settings' ? '返回主界面' : showPanel || wide ? '收起面板' : '展开面板'}
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
                          title="返回主界面"
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
                        <h2>系统集成</h2>
                        <div className="setting-toggle-grid">
                          <div className="setting-toggle-item">
                            <span className="setting-tip">开机自启动</span>
                            <button
                              type="button"
                              className="switch-btn"
                              aria-label="切换开机自启动"
                              aria-pressed={autoStartEnabled}
                              title={autoStartEnabled ? '关闭开机自启动' : '开启开机自启动'}
                              disabled={savingSystemIntegration}
                              onClick={async () => {
                                const next = !autoStartEnabled
                                await saveSystemIntegration(next, contextMenuEnabled)
                              }}
                            >
                              <span className="switch-thumb" />
                            </button>
                          </div>
                          <div className="setting-toggle-item">
                            <span className="setting-tip">加入右键菜单栏</span>
                            <button
                              type="button"
                              className="switch-btn"
                              aria-label="切换右键菜单栏"
                              aria-pressed={contextMenuEnabled}
                              title={contextMenuEnabled ? '关闭右键菜单栏' : '开启右键菜单栏'}
                              disabled={savingSystemIntegration}
                              onClick={async () => {
                                const next = !contextMenuEnabled
                                await saveSystemIntegration(autoStartEnabled, next)
                              }}
                            >
                              <span className="switch-thumb" />
                            </button>
                          </div>
                        </div>
                        <div className="idle-hide-row">
                          <span className="setting-tip">最小岛无交互自动隐藏</span>
                          <select
                            className="idle-hide-select"
                            value={idleHideSeconds}
                            disabled={savingIdleHide}
                            onChange={(event) => {
                              const next = Number(event.target.value)
                              void saveIdleHideSeconds(next)
                            }}
                          >
                            {IDLE_HIDE_OPTIONS.map((option) => (
                              <option key={`idle-${option.value}`} value={option.value}>{option.label}</option>
                            ))}
                          </select>
                        </div>
                      </section>

                      <section className="settings-card panel-settings-card">
                        <div className="data-dir-head">
                          <div className="data-dir-title-wrap">
                            <h2>用户数据目录</h2>
                            <p>用于存放 scripts 与 logs。默认路径在 AppData，可手动改到其他盘符。</p>
                          </div>
                          <div className="data-dir-head-actions">
                            <button
                              type="button"
                              className={`script-icon-btn ${actionFeedbackKey === 'open-user-data' ? 'is-pressed' : ''}`}
                              aria-label="打开用户数据目录"
                              title="打开用户数据目录"
                              disabled={openingUserDataFolder || savingDataDirectory || pickingDataDirectory}
                              onClick={() => {
                                flashActionFeedback('open-user-data')
                                void openUserDataFolder()
                              }}
                            >
                              <svg viewBox="0 0 24 24" aria-hidden="true">
                                <path
                                  fill="currentColor"
                                  d="M14 2H7a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V7l-5-5Zm0 2.5L16.5 7H14V4.5ZM9 11h6a1 1 0 1 1 0 2H9a1 1 0 1 1 0-2Zm0 4h6a1 1 0 1 1 0 2H9a1 1 0 1 1 0-2Z"
                                />
                              </svg>
                            </button>
                          </div>
                        </div>
                        <div className="data-dir-row">
                          <input
                            className="data-dir-input"
                            type="text"
                            value={dataDirectoryInput}
                            placeholder="例如 D:/OmniIsleData"
                            readOnly
                            disabled={savingDataDirectory || pickingDataDirectory}
                          />
                          <button
                            type="button"
                            className="env-action-btn"
                            aria-label="选择数据目录"
                            title="选择目录"
                            disabled={savingDataDirectory || pickingDataDirectory}
                            onClick={() => void pickDataDirectory()}
                          >
                            <svg viewBox="0 0 24 24" aria-hidden="true">
                              <path
                                fill="currentColor"
                                d="M10 4a2 2 0 0 1 1.4.6l1 1H19a2 2 0 0 1 2 2v8.5A2.5 2.5 0 0 1 18.5 18h-13A2.5 2.5 0 0 1 3 15.5v-9A2.5 2.5 0 0 1 5.5 4H10Zm0 2H5.5a.5.5 0 0 0-.5.5v9c0 .28.22.5.5.5h13a.5.5 0 0 0 .5-.5V8a.5.5 0 0 0-.5-.5h-6.93a2 2 0 0 1-1.4-.58L10 6Z"
                              />
                            </svg>
                          </button>
                          <button
                            type="button"
                            className="env-action-btn env-action-btn-danger"
                            aria-label="恢复默认数据目录"
                            title="恢复默认目录"
                            disabled={savingDataDirectory || pickingDataDirectory}
                            onClick={() => void saveDataDirectory('')}
                          >
                            <svg viewBox="0 0 24 24" aria-hidden="true">
                              <path
                                fill="currentColor"
                                d="M12 4a8 8 0 1 1-7.75 10h2.07A6 6 0 1 0 8 8.52V11H2V5h2v1.72A7.96 7.96 0 0 1 12 4Z"
                              />
                            </svg>
                          </button>
                        </div>
                      </section>

                      <section className="settings-card panel-settings-card">
                        <h2>运行环境</h2>
                        <p>每行一个运行环境，可新增更多环境并在系统中检索默认命令。</p>
                        <div className="env-config-list">
                          {envConfigs.map((row) => (
                            <div key={`env-${row.id}`} className="env-config-row">
                              <select
                                className="env-name-select"
                                value={row.name}
                                disabled={savingEnvConfigs || pickingEnvRowId === row.id}
                                onChange={async (event) => {
                                  const nextName = event.target.value
                                  await updateEnvConfigRow(row.id, { name: nextName })
                                }}
                              >
                                {envNameOptions.map((option) => (
                                  <option key={option} value={option}>{option}</option>
                                ))}
                              </select>
                              <input
                                className="env-value-input"
                                type="text"
                                value={row.value}
                                placeholder="请使用右侧搜索图标选择运行环境路径"
                                readOnly
                                disabled={savingEnvConfigs || pickingEnvRowId === row.id}
                              />
                              <button
                                type="button"
                                className="env-action-btn"
                                aria-label="系统检索"
                                title="系统检索"
                                disabled={savingEnvConfigs || pickingEnvRowId === row.id}
                                onClick={async () => resolveEnvConfigRow(row)}
                              >
                                <svg viewBox="0 0 24 24" aria-hidden="true">
                                  <path
                                    fill="currentColor"
                                    d="M10.5 3a7.5 7.5 0 1 0 4.86 13.22l4.2 4.2a1 1 0 0 0 1.42-1.42l-4.2-4.2A7.5 7.5 0 0 0 10.5 3Zm0 2a5.5 5.5 0 1 1 0 11 5.5 5.5 0 0 1 0-11Z"
                                  />
                                </svg>
                              </button>
                              <button
                                type="button"
                                className="env-action-btn env-action-btn-danger"
                                aria-label="删除环境配置"
                                title="删除"
                                onClick={async () => removeEnvConfigRow(row.id)}
                                disabled={envConfigs.length <= 1 || savingEnvConfigs || pickingEnvRowId === row.id}
                              >
                                <svg viewBox="0 0 24 24" aria-hidden="true">
                                  <path
                                    fill="currentColor"
                                    d="M9 3a1 1 0 0 0-1 1v1H5a1 1 0 1 0 0 2h1l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12h1a1 1 0 1 0 0-2h-3V4a1 1 0 0 0-1-1H9Zm2 2h2v1h-2V5Zm-2 4a1 1 0 0 1 1 1v7a1 1 0 1 1-2 0v-7a1 1 0 0 1 1-1Zm6 0a1 1 0 0 1 1 1v7a1 1 0 1 1-2 0v-7a1 1 0 0 1 1-1Z"
                                  />
                                </svg>
                              </button>
                            </div>
                          ))}
                        </div>
                        <button
                          type="button"
                          className="ghost-btn"
                          onClick={addEnvConfigRow}
                          aria-label="添加运行环境"
                          title="添加运行环境"
                          disabled={savingEnvConfigs || pickingEnvRowId !== null}
                        >
                          + 添加运行环境
                        </button>
                      </section>

                      <section className="settings-card panel-settings-card">
                        <div className="script-manage-head">
                          <h2>脚本管理</h2>
                          <button
                            type="button"
                            className={`script-icon-btn ${refreshingScripts ? 'is-working' : ''}`}
                            aria-label="刷新脚本列表"
                            title="刷新脚本列表并同步右键菜单"
                            onClick={() => void refreshScriptCatalog()}
                            disabled={savingScripts || refreshingScripts}
                          >
                            <svg viewBox="0 0 24 24" aria-hidden="true">
                              <path
                                fill="currentColor"
                                d="M12 5a7 7 0 0 1 6.32 4H16a1 1 0 1 0 0 2h4.5A1.5 1.5 0 0 0 22 9.5V5a1 1 0 1 0-2 0v1.44A9 9 0 1 0 21 12a1 1 0 1 0-2 0 7 7 0 1 1-7-7Z"
                              />
                            </svg>
                          </button>
                        </div>
                        <p>支持新增、编辑、删除；运行时根据脚本类型自动匹配运行环境。</p>
                        <ul className="script-list">
                          {scriptItems.length === 0 && (
                            <li>
                              <div className="script-label">
                                <span className="script-name">暂无脚本</span>
                                <span className="script-file">请点击下方“添加脚本”</span>
                              </div>
                            </li>
                          )}
                          {scriptItems.map((item) => (
                            <li key={`manage-${item.script}`}>
                              <div className="script-label" title={`${item.label}\n${item.script}`}>
                                <span className="script-name">{item.label}</span>
                                <span className="script-file">{item.script}</span>
                              </div>
                              <div className="script-actions">
                                <button
                                  type="button"
                                  className={`script-icon-btn ${actionFeedbackKey === `run-${item.script}` ? 'is-pressed' : ''}`}
                                  onClick={() => queueScriptRun(item)}
                                  aria-label="运行脚本"
                                  title="运行"
                                  disabled={savingScripts}
                                >
                                  <svg viewBox="0 0 24 24" aria-hidden="true">
                                    <path fill="currentColor" d="M8 5v14l11-7-11-7Z" />
                                  </svg>
                                </button>
                                <button
                                  type="button"
                                  className={`script-icon-btn ${actionFeedbackKey === `edit-${item.script}` ? 'is-pressed' : ''} ${editingScriptName === item.script ? 'is-working' : ''}`}
                                  onClick={() => void onEditScript(item)}
                                  aria-label="编辑脚本"
                                  title="编辑"
                                  disabled={savingScripts || editingScriptName === item.script}
                                >
                                  <svg viewBox="0 0 24 24" aria-hidden="true">
                                    <path
                                      fill="currentColor"
                                      d="M3 17.25V21h3.75L17.8 9.94l-3.75-3.75L3 17.25Zm17.71-10.04a1.003 1.003 0 0 0 0-1.42l-2.5-2.5a1.003 1.003 0 0 0-1.42 0l-1.96 1.96 3.75 3.75 2.13-1.79Z"
                                    />
                                  </svg>
                                </button>
                                <button type="button" className="script-icon-btn script-icon-btn-danger" onClick={() => requestDeleteScript(item)} aria-label="删除脚本" title="删除" disabled={savingScripts}>
                                  <svg viewBox="0 0 24 24" aria-hidden="true">
                                    <path
                                      fill="currentColor"
                                      d="M9 3a1 1 0 0 0-1 1v1H5a1 1 0 1 0 0 2h1l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12h1a1 1 0 1 0 0-2h-3V4a1 1 0 0 0-1-1H9Zm2 2h2v1h-2V5Zm-2 4a1 1 0 0 1 1 1v7a1 1 0 1 1-2 0v-7a1 1 0 0 1 1-1Zm6 0a1 1 0 0 1 1 1v7a1 1 0 1 1-2 0v-7a1 1 0 0 1 1-1Z"
                                    />
                                  </svg>
                                </button>
                              </div>
                            </li>
                          ))}
                        </ul>
                        {pendingDeleteScript && (
                          <div className="script-delete-confirm" role="alertdialog" aria-live="assertive" aria-label="确认删除脚本">
                            <p>
                              确认删除脚本 <strong>{pendingDeleteScript.script}</strong> 吗？
                            </p>
                            <span>将同时删除配置项与脚本文件。</span>
                            <div className="script-create-actions">
                              <button type="button" className="script-create-btn" onClick={cancelDeleteScript} disabled={savingScripts} aria-label="取消删除脚本" title="取消删除脚本">
                                取消
                              </button>
                              <button type="button" className="script-create-btn script-delete-confirm-btn" onClick={() => void confirmDeleteScript()} disabled={savingScripts} aria-label="确认删除脚本" title="确认删除脚本">
                                {savingScripts ? '删除中...' : '确认删除'}
                              </button>
                            </div>
                          </div>
                        )}
                        {showAddScriptForm && (
                          <div className="script-create-panel">
                            <label className="script-create-field">
                              <span>脚本文件名</span>
                              <input
                                className="script-create-input"
                                type="text"
                                value={newScriptName}
                                placeholder="如 task.py / task.cpp / task.js / task.java"
                                disabled={savingScripts}
                                onChange={(event) => setNewScriptName(event.target.value)}
                              />
                            </label>
                            <label className="script-create-field">
                              <span>显示名称</span>
                              <input
                                className="script-create-input"
                                type="text"
                                value={newScriptLabel}
                                placeholder="可留空，默认使用文件名"
                                disabled={savingScripts}
                                onChange={(event) => setNewScriptLabel(event.target.value)}
                              />
                            </label>
                            <div className="script-create-actions">
                              <button type="button" className="script-create-btn" onClick={cancelAddScript} disabled={savingScripts} aria-label="取消新增脚本" title="取消新增脚本">
                                取消
                              </button>
                              <button type="button" className="script-create-btn script-create-btn-primary" onClick={() => void onAddScript()} disabled={savingScripts} aria-label="创建并编辑脚本" title="创建并编辑脚本">
                                {savingScripts ? '创建中...' : '创建并编辑'}
                              </button>
                            </div>
                          </div>
                        )}
                        <button
                          type="button"
                          className="ghost-btn"
                          onClick={showAddScriptForm ? cancelAddScript : startAddScript}
                          aria-label={showAddScriptForm ? '取消新增脚本' : '添加脚本'}
                          title={showAddScriptForm ? '取消新增脚本' : '添加脚本'}
                          disabled={savingScripts}
                        >
                          {showAddScriptForm ? '取消新增' : '+ 添加脚本'}
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
                            <button
                              type="button"
                              className="github-link compact"
                              aria-label="打开 Github 仓库"
                              title="打开 Github 仓库"
                              onClick={() => void invoke('open_url', { url: GITHUB_REPO_URL })}
                            >
                              <svg viewBox="0 0 24 24" aria-hidden="true">
                                <path
                                  fill="currentColor"
                                  d="M12 2C6.48 2 2 6.58 2 12.22c0 4.5 2.87 8.32 6.84 9.66.5.1.68-.22.68-.49 0-.24-.01-1.03-.01-1.86-2.78.62-3.37-1.21-3.37-1.21-.45-1.2-1.11-1.52-1.11-1.52-.91-.64.07-.63.07-.63 1 .08 1.53 1.05 1.53 1.05.9 1.57 2.36 1.12 2.93.86.09-.67.35-1.12.63-1.38-2.22-.26-4.56-1.14-4.56-5.06 0-1.12.39-2.04 1.03-2.75-.1-.26-.45-1.31.1-2.72 0 0 .84-.27 2.75 1.05A9.28 9.28 0 0 1 12 6.77a9.3 9.3 0 0 1 2.5.35c1.9-1.32 2.74-1.05 2.74-1.05.55 1.41.2 2.46.1 2.72.64.71 1.03 1.63 1.03 2.75 0 3.93-2.34 4.8-4.57 5.05.36.31.67.92.67 1.86 0 1.34-.01 2.42-.01 2.74 0 .27.18.59.69.49A10.25 10.25 0 0 0 22 12.22C22 6.58 17.52 2 12 2Z"
                                />
                              </svg>
                            </button>
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
                        title="进入设置"
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
