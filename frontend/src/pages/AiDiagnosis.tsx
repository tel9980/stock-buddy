import { useState, useRef, useEffect } from 'react';
import { useSearchParams } from 'react-router-dom';
import MarkdownView from '../components/MarkdownView';
import { api } from '../api';
import { usePersistedState } from '../stores/persistedState';

type AnalysisType = 'diagnose' | 'predict';

export default function AiDiagnosis() {
  const [searchParams] = useSearchParams();
  const [code, setCode] = usePersistedState('ai.diagnosis.code', '');
  const [content, setContent] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [aiConfigured, setAiConfigured] = useState(true);
  const [activeTab, setActiveTab] = usePersistedState<AnalysisType>('ai.diagnosis.tab', 'diagnose');
  const scrollRef = useRef<HTMLDivElement>(null);
  const autoRunRef = useRef(false);

  useEffect(() => {
    api.getAiStatus().then(s => setAiConfigured(s.configured)).catch(() => {});
  }, []);

  // 从 URL 参数读取股票代码，自动触发诊断（仅首次）
  useEffect(() => {
    const queryCode = searchParams.get('code');
    if (queryCode && !autoRunRef.current) {
      autoRunRef.current = true;
      setCode(queryCode);
      runAnalysis('diagnose', queryCode);
    }
  }, [searchParams]);

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
    }
  }, [content]);

  const runAnalysis = async (type: AnalysisType, overrideCode?: string) => {
    const targetCode = overrideCode ?? code;
    if (!targetCode.trim()) {
      setError('请输入股票代码');
      return;
    }
    if (!aiConfigured) {
      setError('AI 未配置，请在 .env 中设置 LLM_API_KEY、LLM_BASE_URL、LLM_MODEL');
      return;
    }

    setLoading(true);
    setError('');
    setContent('');
    setActiveTab(type);

    const onChunk = (text: string) => {
      setContent(prev => prev + text);
    };

    if (type === 'diagnose') {
      api.diagnoseStockStream(targetCode.trim(), onChunk, () => setLoading(false), (e) => {
        setError(e);
        setLoading(false);
      });
    } else {
      api.predictTrendStream(targetCode.trim(), onChunk, () => setLoading(false), (e) => {
        setError(e);
        setLoading(false);
      });
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: '16px' }}>
      <div style={{ display: 'flex', gap: '12px', alignItems: 'center' }}>
        <input
          value={code}
          onChange={e => setCode(e.target.value)}
          onKeyDown={e => e.key === 'Enter' && !loading && runAnalysis(activeTab)}
          placeholder="输入股票代码，如 000001"
          style={{
            flex: 1, padding: '10px 16px', borderRadius: '8px',
            border: '1px solid var(--border-color, #3a3a4e)',
            background: 'var(--input-bg, #1e1e2e)', color: 'var(--text-primary, #e0e0e8)',
            fontSize: '14px', outline: 'none',
          }}
        />
        <button
          className={`ai-tab-btn ${activeTab === 'diagnose' ? 'active' : ''}`}
          onClick={() => runAnalysis('diagnose')}
          disabled={loading}
        >
          AI 诊断
        </button>
        <button
          className={`ai-tab-btn ${activeTab === 'predict' ? 'active' : ''}`}
          onClick={() => runAnalysis('predict')}
          disabled={loading}
        >
          趋势预测
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
            <span>正在采集数据并分析中...</span>
          </div>
        ) : (
          <div className="ai-placeholder">
            <div className="ai-placeholder-icon">🤖</div>
            <p>输入股票代码，点击「AI 诊断」或「趋势预测」</p>
            <p className="ai-placeholder-hint">
              AI 将综合技术指标、K线形态、市场环境等多维数据给出研判
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
