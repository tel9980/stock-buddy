import { useEffect, useState } from 'react'
import { api } from '../api'

/**
 * AI 设置页：在页面上直接填 API Key / Base URL / 模型，保存后立即生效。
 *
 * 后端把配置落盘到 data/ai-config.json（容器里挂在 /app/data 卷上），
 * 所以重启容器也不会丢，无需再碰 docker-compose 的环境变量。
 */
export default function Settings() {
  const [apiKey, setApiKey] = useState('')
  const [baseUrl, setBaseUrl] = useState('')
  const [model, setModel] = useState('')
  const [configured, setConfigured] = useState(false)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null)
  // 用户是否真的改过 key：不改就用打码值回传，后端会保留原 key
  const [keyTouched, setKeyTouched] = useState(false)
  // 「拉取模型」得到的候选列表
  const [models, setModels] = useState<string[]>([])
  const [fetchingModels, setFetchingModels] = useState(false)

  const load = async () => {
    setLoading(true)
    try {
      const cfg = await api.getAiConfig()
      setApiKey(cfg.api_key)
      setBaseUrl(cfg.base_url)
      setModel(cfg.model)
      setKeyTouched(false)
      const st = await api.getAiStatus()
      setConfigured(st.configured)
    } catch (e) {
      setMsg({ ok: false, text: `读取配置失败：${(e as Error).message}` })
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    load()
  }, [])

  /**
   * 拉取上游模型列表。
   *
   * 注意：这一步只用后端**已保存**的 key/base_url —— 页面上刚填还没保存的
   * 改动不会被带上，所以改过地址或 key 时要先「保存并生效」再拉取。
   */
  const fetchModels = async () => {
    setFetchingModels(true)
    setMsg(null)
    try {
      const list = await api.listAiModels()
      setModels(list)
      setMsg({ ok: true, text: `已拉取 ${list.length} 个模型，请在下方下拉框选择` })
    } catch (e) {
      setModels([])
      setMsg({ ok: false, text: `拉取模型失败：${(e as Error).message}` })
    } finally {
      setFetchingModels(false)
    }
  }

  const save = async () => {
    setSaving(true)
    setMsg(null)
    try {
      await api.updateAiConfig({ api_key: apiKey, base_url: baseUrl, model })
      setMsg({ ok: true, text: '已保存并立即生效（无需重启容器）' })
      await load()
    } catch (e) {
      setMsg({ ok: false, text: `保存失败：${(e as Error).message}` })
    } finally {
      setSaving(false)
    }
  }

  const inputStyle: React.CSSProperties = {
    width: '100%',
    padding: '8px 10px',
    borderRadius: 6,
    border: '1px solid var(--border, #d9d9d9)',
    background: 'var(--bg-input, #fff)',
    color: 'inherit',
    fontSize: 14,
  }
  const labelStyle: React.CSSProperties = {
    display: 'block',
    marginBottom: 6,
    fontSize: 13,
    opacity: 0.75,
  }

  return (
    <div style={{ maxWidth: 720, margin: '0 auto', padding: '24px 16px' }}>
      <h2 style={{ marginBottom: 4 }}>AI 设置</h2>
      <p style={{ fontSize: 13, opacity: 0.65, marginBottom: 24 }}>
        兼容任何 OpenAI 格式的接口（OpenAI / DeepSeek / Moonshot / 通义 / 本地 Ollama 等）。
        配置保存后立即生效，并持久化到 data 卷。
      </p>

      <div
        style={{
          padding: '10px 14px',
          borderRadius: 8,
          marginBottom: 20,
          fontSize: 13,
          background: configured ? 'rgba(82,196,26,0.1)' : 'rgba(255,193,7,0.12)',
          border: `1px solid ${configured ? 'rgba(82,196,26,0.35)' : 'rgba(255,193,7,0.35)'}`,
        }}
      >
        {configured ? '✅ AI 已配置，可直接使用 AI 诊断 / 研判 / 问答' : '⚠️ AI 尚未配置，填入 API Key 后保存即可启用'}
      </div>

      {loading ? (
        <div style={{ padding: 40, textAlign: 'center', opacity: 0.6 }}>加载中…</div>
      ) : (
        <div style={{ display: 'grid', gap: 18 }}>
          <div>
            <label style={labelStyle}>API Key</label>
            <input
              style={inputStyle}
              type="password"
              value={apiKey}
              placeholder="sk-..."
              onChange={(e) => {
                setApiKey(e.target.value)
                setKeyTouched(true)
              }}
            />
            {!keyTouched && apiKey.includes('****') && (
              <div style={{ fontSize: 12, opacity: 0.55, marginTop: 6 }}>
                已保存的 Key（打码显示）。不改就留空原样，改了就覆盖。
              </div>
            )}
          </div>

          <div>
            <label style={labelStyle}>Base URL</label>
            <input
              style={inputStyle}
              value={baseUrl}
              placeholder="https://api.openai.com/v1"
              onChange={(e) => setBaseUrl(e.target.value)}
            />
            <div style={{ fontSize: 12, opacity: 0.55, marginTop: 6 }}>
              常见：OpenAI <code>https://api.openai.com/v1</code> · DeepSeek <code>https://api.deepseek.com/v1</code> · Moonshot <code>https://api.moonshot.cn/v1</code> · Ollama <code>http://localhost:11434/v1</code>
            </div>
          </div>

          <div>
            <label style={labelStyle}>模型</label>
            <div style={{ display: 'flex', gap: 8 }}>
              {models.length > 0 ? (
                <select
                  style={{ ...inputStyle, flex: 1 }}
                  value={models.includes(model) ? model : ''}
                  onChange={(e) => setModel(e.target.value)}
                >
                  <option value="">— 从已拉取的模型中选择 —</option>
                  {models.map((m) => (
                    <option key={m} value={m}>
                      {m}
                    </option>
                  ))}
                </select>
              ) : (
                <input
                  style={{ ...inputStyle, flex: 1 }}
                  value={model}
                  placeholder="gpt-4o-mini / deepseek-chat / moonshot-v1-8k"
                  onChange={(e) => setModel(e.target.value)}
                />
              )}
              <button
                onClick={fetchModels}
                disabled={fetchingModels || saving}
                title="用已保存的 Base URL 与 Key 请求 /models，自动列出可用模型"
                style={{
                  padding: '8px 16px',
                  borderRadius: 6,
                  border: '1px solid var(--border, #d9d9d9)',
                  background: 'transparent',
                  color: 'inherit',
                  fontSize: 14,
                  whiteSpace: 'nowrap',
                  cursor: fetchingModels ? 'not-allowed' : 'pointer',
                  opacity: fetchingModels ? 0.6 : 1,
                }}
              >
                {fetchingModels ? '拉取中…' : '拉取模型'}
              </button>
            </div>
            <div style={{ fontSize: 12, opacity: 0.55, marginTop: 6 }}>
              {models.length > 0
                ? '已从上游拉到模型列表，可直接选择；手动输入也行。'
                : '不确定模型名？点「拉取模型」自动列出（先用已保存的地址和 Key）。'}
            </div>
          </div>

          <div style={{ display: 'flex', gap: 12, alignItems: 'center' }}>
            <button
              onClick={save}
              disabled={saving}
              style={{
                padding: '9px 22px',
                borderRadius: 6,
                border: 'none',
                background: '#1677ff',
                color: '#fff',
                fontSize: 14,
                cursor: saving ? 'not-allowed' : 'pointer',
                opacity: saving ? 0.6 : 1,
              }}
            >
              {saving ? '保存中…' : '保存并生效'}
            </button>
            <button
              onClick={load}
              disabled={saving}
              style={{
                padding: '9px 18px',
                borderRadius: 6,
                border: '1px solid var(--border, #d9d9d9)',
                background: 'transparent',
                color: 'inherit',
                fontSize: 14,
                cursor: 'pointer',
              }}
            >
              重新读取
            </button>
          </div>

          {msg && (
            <div
              style={{
                padding: '10px 14px',
                borderRadius: 6,
                fontSize: 13,
                background: msg.ok ? 'rgba(82,196,26,0.1)' : 'rgba(255,77,79,0.1)',
                border: `1px solid ${msg.ok ? 'rgba(82,196,26,0.35)' : 'rgba(255,77,79,0.35)'}`,
              }}
            >
              {msg.text}
            </div>
          )}
        </div>
      )}
    </div>
  )
}
