import { useEffect, useState } from 'react'
import { api } from '../api'
import type { FuyaoCfg, LlmCfg, MxCfg, ProvidersView } from '../api'

/**
 * 设置页：三块数据源，各自「填写 → 测试连通 → 保存即生效」。
 *
 * | 来源 | 作用 | 鉴权 |
 * |---|---|---|
 * | 大模型（LLM） | AI 诊断/研判/问答的主引擎（OpenAI 兼容） | `Authorization: Bearer` |
 * | 东方财富妙想 | 妙想没配 LLM 时接管 AI；也可做数据查询/选股 | `em_api_key` |
 * | 同花顺 fuyao | 涨停池、龙虎榜等第三方数据 | `X-api-key` |
 *
 * key 一律由后端打码回传（`sk-a****nop`）。只要不动输入框，回传的就是打码值，
 * 后端据此保留原 key —— 所以「只改模型名」不会把 key 洗掉。
 */

type TestState = { ok: boolean; text: string } | null

/** 卡片外壳，三块共用，保证视觉一致 */
function Card(props: {
  title: string
  tag: string
  configured: boolean
  desc: string
  children: React.ReactNode
}) {
  return (
    <section className="rounded-xl border border-slate-200 bg-white p-5 shadow-sm dark:border-slate-700 dark:bg-slate-900">
      <header className="mb-4 flex items-start justify-between gap-3">
        <div>
          <h2 className="flex items-center gap-2 text-base font-semibold text-slate-800 dark:text-slate-100">
            {props.title}
            <span className="rounded bg-slate-100 px-1.5 py-0.5 text-[11px] font-normal text-slate-500 dark:bg-slate-800 dark:text-slate-400">
              {props.tag}
            </span>
          </h2>
          <p className="mt-1 text-xs leading-relaxed text-slate-500 dark:text-slate-400">
            {props.desc}
          </p>
        </div>
        <span
          className={`shrink-0 rounded-full px-2 py-0.5 text-[11px] ${
            props.configured
              ? 'bg-emerald-50 text-emerald-600 dark:bg-emerald-900/30 dark:text-emerald-400'
              : 'bg-slate-100 text-slate-500 dark:bg-slate-800 dark:text-slate-400'
          }`}
        >
          {props.configured ? '已配置' : '未配置'}
        </span>
      </header>
      {props.children}
    </section>
  )
}

function Field(props: {
  label: string
  value: string
  onChange: (v: string) => void
  placeholder?: string
  type?: string
  hint?: React.ReactNode
}) {
  return (
    <label className="mb-3 block">
      <span className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-300">
        {props.label}
      </span>
      <input
        type={props.type ?? 'text'}
        value={props.value}
        onChange={(e) => props.onChange(e.target.value)}
        placeholder={props.placeholder}
        className="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-800 outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500 dark:border-slate-600 dark:bg-slate-800 dark:text-slate-100"
      />
      {props.hint && (
        <span className="mt-1 block text-[11px] leading-relaxed text-slate-400">{props.hint}</span>
      )}
    </label>
  )
}

/** 一行「测试 / 结果」 */
function TestRow(props: {
  testing: boolean
  result: TestState
  onTest: () => void
  label?: string
}) {
  return (
    <div className="mt-1 flex items-center gap-3">
      <button
        type="button"
        onClick={props.onTest}
        disabled={props.testing}
        className="rounded-lg border border-slate-300 px-3 py-1.5 text-xs text-slate-700 hover:bg-slate-50 disabled:opacity-50 dark:border-slate-600 dark:text-slate-200 dark:hover:bg-slate-800"
      >
        {props.testing ? '测试中…' : (props.label ?? '测试连接')}
      </button>
      {props.result && (
        <span
          className={`text-xs ${props.result.ok ? 'text-emerald-600 dark:text-emerald-400' : 'text-red-500'}`}
        >
          {props.result.text}
        </span>
      )}
    </div>
  )
}

export default function Settings() {
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [msg, setMsg] = useState<TestState>(null)

  // 三套配置的编辑态
  const [llm, setLlm] = useState<LlmCfg>({ api_key: '', base_url: '', model: '' })
  const [mx, setMx] = useState<MxCfg>({ api_key: '', base_url: '' })
  const [fuyao, setFuyao] = useState<FuyaoCfg>({ api_key: '' })
  const [status, setStatus] = useState({ llm: false, mx: false, fuyao: false })
  const [engine, setEngine] = useState('none')

  const [models, setModels] = useState<string[]>([])
  const [loadingModels, setLoadingModels] = useState(false)
  const [modelsErr, setModelsErr] = useState('')

  const [testing, setTesting] = useState<'' | 'llm' | 'mx' | 'fuyao'>('')
  const [testResult, setTestResult] = useState<Record<string, TestState>>({})

  const apply = (v: ProvidersView) => {
    setLlm(v.llm)
    setMx(v.mx)
    setFuyao(v.fuyao)
    setStatus(v.status)
  }

  const load = async () => {
    setLoading(true)
    try {
      apply(await api.getProviders())
      const st = await api.getAiStatus()
      setEngine(st.engine)
    } catch (e) {
      setMsg({ ok: false, text: `读取配置失败：${(e as Error).message}` })
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    load()
  }, [])

  const save = async () => {
    setSaving(true)
    setMsg(null)
    try {
      const v = await api.updateProviders({ llm, mx, fuyao })
      apply(v)
      const st = await api.getAiStatus()
      setEngine(st.engine)
      setMsg({ ok: true, text: '已保存并立即生效' })
    } catch (e) {
      setMsg({ ok: false, text: `保存失败：${(e as Error).message}` })
    } finally {
      setSaving(false)
    }
  }

  /** 拉取上游可用模型。用页面上**当前填写**的 key/base_url，不用先保存。 */
  const loadModels = async () => {
    setLoadingModels(true)
    setModelsErr('')
    try {
      const list = await api.listAiModels({ api_key: llm.api_key, base_url: llm.base_url })
      setModels(list)
      if (list.length === 0) setModelsErr('上游没有返回任何模型')
    } catch (e) {
      setModelsErr((e as Error).message)
      setModels([])
    } finally {
      setLoadingModels(false)
    }
  }

  const runTest = async (kind: 'llm' | 'mx' | 'fuyao') => {
    setTesting(kind)
    setTestResult((r) => ({ ...r, [kind]: null }))
    try {
      const body =
        kind === 'llm'
          ? { api_key: llm.api_key, base_url: llm.base_url, model: llm.model }
          : kind === 'mx'
            ? { api_key: mx.api_key, base_url: mx.base_url }
            : { api_key: fuyao.api_key }
      const res = await api.testProvider(kind, body)
      setTestResult((r) => ({
        ...r,
        [kind]: {
          ok: true,
          text: res.latency_ms ? `${res.message}（${res.latency_ms}ms）` : res.message,
        },
      }))
    } catch (e) {
      setTestResult((r) => ({ ...r, [kind]: { ok: false, text: (e as Error).message } }))
    } finally {
      setTesting('')
    }
  }

  if (loading) {
    return <div className="p-6 text-sm text-slate-500">读取配置中…</div>
  }

  const engineLabel =
    engine === 'llm' ? '大模型' : engine === 'mx' ? '东方财富妙想' : '未启用'

  return (
    <div className="mx-auto max-w-3xl space-y-4 p-4 pb-16">
      <div className="flex items-end justify-between">
        <div>
          <h1 className="text-xl font-semibold text-slate-800 dark:text-slate-100">设置</h1>
          <p className="mt-1 text-xs text-slate-500 dark:text-slate-400">
            当前 AI 引擎：<span className="font-medium text-slate-700 dark:text-slate-200">{engineLabel}</span>
            {engine === 'mx' && '（未配置大模型，自动回落妙想）'}
          </p>
        </div>
        <button
          type="button"
          onClick={save}
          disabled={saving}
          className="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white hover:bg-blue-700 disabled:opacity-50"
        >
          {saving ? '保存中…' : '保存并生效'}
        </button>
      </div>

      {msg && (
        <div
          className={`rounded-lg px-3 py-2 text-xs ${
            msg.ok
              ? 'bg-emerald-50 text-emerald-700 dark:bg-emerald-900/30 dark:text-emerald-300'
              : 'bg-red-50 text-red-600 dark:bg-red-900/30 dark:text-red-300'
          }`}
        >
          {msg.text}
        </div>
      )}

      <Card
        title="大模型"
        tag="OpenAI 兼容"
        configured={status.llm}
        desc="AI 个股诊断、市场研判、趋势预测、智能问答的主引擎。支持 OpenAI / DeepSeek / Moonshot / 本地 vLLM（如 http://192.168.2.22:2682/v1）。"
      >
        <Field
          label="API Key"
          type="password"
          value={llm.api_key}
          onChange={(v) => setLlm({ ...llm, api_key: v })}
          placeholder="sk-…"
          hint="保存后只显示打码值；不动输入框则保留原 Key。"
        />
        <Field
          label="Base URL"
          value={llm.base_url}
          onChange={(v) => setLlm({ ...llm, base_url: v })}
          placeholder="https://api.openai.com/v1"
          hint="本地网关填到 /v1 为止，例如 http://192.168.2.22:2682/v1"
        />
        <label className="mb-3 block">
          <span className="mb-1 block text-xs font-medium text-slate-600 dark:text-slate-300">
            模型
          </span>
          <div className="flex gap-2">
            <input
              type="text"
              value={llm.model}
              onChange={(e) => setLlm({ ...llm, model: e.target.value })}
              placeholder="gpt-4o-mini"
              className="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-800 outline-none focus:border-blue-500 focus:ring-1 focus:ring-blue-500 dark:border-slate-600 dark:bg-slate-800 dark:text-slate-100"
            />
            <button
              type="button"
              onClick={loadModels}
              disabled={loadingModels}
              className="shrink-0 rounded-lg border border-slate-300 px-3 py-2 text-xs text-slate-700 hover:bg-slate-50 disabled:opacity-50 dark:border-slate-600 dark:text-slate-200 dark:hover:bg-slate-800"
            >
              {loadingModels ? '拉取中…' : '拉取模型'}
            </button>
          </div>
          {models.length > 0 ? (
            <div className="mt-2">
              <select
                value={models.includes(llm.model) ? llm.model : ''}
                onChange={(e) => e.target.value && setLlm({ ...llm, model: e.target.value })}
                className="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-800 dark:border-slate-600 dark:bg-slate-800 dark:text-slate-100"
              >
                <option value="">从 {models.length} 个可用模型中选择…</option>
                {models.map((m) => (
                  <option key={m} value={m}>
                    {m}
                  </option>
                ))}
              </select>
            </div>
          ) : (
            <span className="mt-1 block text-[11px] leading-relaxed text-slate-400">
              {modelsErr
                ? `拉取失败：${modelsErr}`
                : '填错模型名通常报 404。可点「拉取模型」从上游列表里选。'}
            </span>
          )}
        </label>
        <TestRow
          testing={testing === 'llm'}
          result={testResult.llm ?? null}
          onTest={() => runTest('llm')}
        />
      </Card>

      <Card
        title="东方财富妙想"
        tag="MXClaw"
        configured={status.mx}
        desc="妙想金融大模型。既可以做大模型没配时的 AI 兜底，也能做自然语言数据查询与选股。注意它不走 OpenAI 协议。"
      >
        <Field
          label="妙想 API Key"
          type="password"
          value={mx.api_key}
          onChange={(v) => setMx({ ...mx, api_key: v })}
          placeholder="在 ai.eastmoney.com/mxClaw 申请"
          hint={
            <>
              申请地址{' '}
              <a
                href="https://ai.eastmoney.com/mxClaw"
                target="_blank"
                rel="noreferrer"
                className="text-blue-600 hover:underline"
              >
                ai.eastmoney.com/mxClaw
              </a>
            </>
          }
        />
        <Field
          label="Base URL"
          value={mx.base_url}
          onChange={(v) => setMx({ ...mx, base_url: v })}
          placeholder="https://ai-saas.eastmoney.com/proxy"
          hint="一般不用改。鉴权头是 em_api_key，与上面的大模型是两套凭证。"
        />
        <TestRow
          testing={testing === 'mx'}
          result={testResult.mx ?? null}
          onTest={() => runTest('mx')}
          label="测一次问答"
        />
      </Card>

      <Card
        title="同花顺数据"
        tag="fuyao"
        configured={status.fuyao}
        desc="涨停池 / 跌停池 / 炸板池 / 龙虎榜的第三方数据源，东财取不到时用它兜底。"
      >
        <Field
          label="fuyao API Key"
          type="password"
          value={fuyao.api_key}
          onChange={(v) => setFuyao({ api_key: v })}
          placeholder="sk-fuyao-…"
          hint="鉴权头是 X-api-key。留空则完全不走这个源。"
        />
        <TestRow
          testing={testing === 'fuyao'}
          result={testResult.fuyao ?? null}
          onTest={() => runTest('fuyao')}
          label="测涨停池"
        />
      </Card>

      <p className="px-1 text-[11px] leading-relaxed text-slate-400">
        配置保存在容器的 <code>data/providers.json</code>（挂在数据卷上），重启不会丢。
        环境变量 LLM_API_KEY / EM_API_KEY / FUYAO_API_KEY 仍作为兜底，页面保存的值优先。
      </p>
    </div>
  )
}
