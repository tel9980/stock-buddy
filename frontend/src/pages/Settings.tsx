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
            <input
              style={inputStyle}
              value={model}
              placeholder="gpt-4o-mini / deepseek-chat / moonshot-v1-8k"
              onChange={(e) => setModel(e.target.value)}
            />
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
