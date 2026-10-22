import { useState, useRef, useEffect } from 'react';
import MarkdownView from '../components/MarkdownView';
import { api } from '../api';

export default function AiMarket() {
  const [content, setContent] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [aiConfigured, setAiConfigured] = useState(true);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api.getAiStatus().then(s => setAiConfigured(s.configured)).catch(() => {});
  }, []);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [content]);

  const runAnalysis = () => {
    if (!aiConfigured) {
      setError('AI 未配置，请在 .env 中设置 LLM_API_KEY、LLM_BASE_URL、LLM_MODEL');
      return;
    }

    setLoading(true);
    setError('');
    setContent('');

    api.marketAnalysisStream(
      (text) => setContent(prev => prev + text),
      () => setLoading(false),
      (e) => { setError(e); setLoading(false); },
    );
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: '16px' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <h2 style={{ margin: 0 }}>AI 市场研判</h2>
        <button
          className="ai-tab-btn active"
          onClick={runAnalysis}
          disabled={loading}
        >
          {loading ? '研判中...' : '开始研判'}
        </button>
      </div>

      {!aiConfigured && (
        <div style={{
          padding: '12px 16px', borderRadius: '8px',
          background: 'rgba(255,193,7,0.1)', border: '1px solid rgba(255,193,7,0.3)',
          color: '#ffc107', fontSize: '13px',
        }}>
          AI 未配置 — 请在后端 .env 中设置 LLM_API_KEY / LLM_BASE_URL / LLM_MODEL 后重启服务
        </div>
      )}

      {error && (
        <div style={{
          padding: '12px 16px', borderRadius: '8px',
          background: 'rgba(244,67,54,0.1)', border: '1px solid rgba(244,67,54,0.3)',
          color: '#f44336', fontSize: '13px',
        }}>
          {error}
        </div>
      )}

      <div ref={scrollRef} className="ai-report-container">
        {content ? (
          <div style={{ position: 'relative' }}>
            <MarkdownView content={content} />
            {loading && <span className="ai-cursor">▊</span>}
          </div>
        ) : loading ? (
          <div className="ai-loading">
            <div className="ai-loading-spinner" />
            <span>正在采集市场数据并分析中...</span>
          </div>
        ) : (
          <div className="ai-placeholder">
            <div className="ai-placeholder-icon">📊</div>
            <p>点击「开始研判」，AI 将分析大盘走势、板块轮动和资金方向</p>
            <p className="ai-placeholder-hint">
              研判内容包括：市场情绪定调、指数分析、板块轮动、涨跌停分析、短期展望、操作策略
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
