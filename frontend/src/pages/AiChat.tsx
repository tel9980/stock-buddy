import { useState, useRef, useEffect } from 'react';
import MarkdownView from '../components/MarkdownView';
import { api } from '../api';
import { usePersistedState } from '../stores/persistedState';

interface Message {
  role: 'user' | 'assistant';
  content: string;
}

export default function AiChat() {
  const [messages, setMessages] = useState<Message[]>([]);
  // 只持久化输入框草稿；对话记录由后端 ai_history 落库，不重复存 localStorage
  const [input, setInput] = usePersistedState('ai.chat.draft', '');
  const [loading, setLoading] = useState(false);
  const [streamingContent, setStreamingContent] = useState('');
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
  }, [messages, streamingContent]);

  const send = () => {
    if (!input.trim() || loading) return;
    if (!aiConfigured) {
      setError('AI 未配置，请在 .env 中设置 LLM_API_KEY');
      return;
    }

    const question = input.trim();
    setInput('');
    setError('');
    setMessages(prev => [...prev, { role: 'user', content: question }]);
    setLoading(true);
    setStreamingContent('');

    api.chatStream(
      question,
      (text) => setStreamingContent(prev => prev + text),
      () => {
        setStreamingContent(prev => {
          setMessages(msgs => [...msgs, { role: 'assistant', content: prev }]);
          return '';
        });
        setLoading(false);
      },
      (e) => {
        setError(e);
        setLoading(false);
        setStreamingContent('');
      },
    );
  };

  const quickQuestions = [
    '今天大盘怎么样？',
    '000001 平安银行走势如何？',
    '当前市场有什么热点板块？',
    '600519 贵州茅台可以买入吗？',
  ];

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: '12px' }}>
      {!aiConfigured && (
        <div style={{
          padding: '10px 16px', borderRadius: '8px',
          background: 'rgba(255,193,7,0.1)', border: '1px solid rgba(255,193,7,0.3)',
          color: '#ffc107', fontSize: '13px',
        }}>
          AI 未配置 — 请在后端 .env 中设置 LLM_API_KEY / LLM_BASE_URL / LLM_MODEL
        </div>
      )}

      <div ref={scrollRef} className="chat-container">
        {messages.length === 0 && !streamingContent ? (
          <div className="ai-placeholder">
            <div className="ai-placeholder-icon">💬</div>
            <p>向 AI 投资顾问提问</p>
            <div style={{ display: 'flex', flexDirection: 'column', gap: '8px', marginTop: '16px' }}>
              {quickQuestions.map(q => (
                <button
                  key={q}
                  className="quick-question-btn"
                  onClick={() => { setInput(q); }}
                >
                  {q}
                </button>
              ))}
            </div>
          </div>
        ) : (
          <>
            {messages.map((msg, i) => (
              <div key={i} className={`chat-msg ${msg.role}`}>
                <div className="chat-msg-role">
                  {msg.role === 'user' ? '🧑 我' : '🤖 AI 顾问'}
                </div>
                <div className="chat-msg-content">
                  <MarkdownView content={msg.content} />
                </div>
              </div>
            ))}
            {streamingContent && (
              <div className="chat-msg assistant">
                <div className="chat-msg-role">🤖 AI 顾问</div>
                <div className="chat-msg-content">
                  <MarkdownView content={streamingContent} />
                  <span className="ai-cursor">▊</span>
                </div>
              </div>
            )}
            {loading && !streamingContent && (
              <div className="chat-msg assistant">
                <div className="chat-msg-role">🤖 AI 顾问</div>
                <div className="chat-msg-content">
                  <div className="ai-loading-inline">
                    <div className="ai-loading-spinner" />
                    <span>正在分析...</span>
                  </div>
                </div>
              </div>
            )}
          </>
        )}
      </div>

      {error && (
        <div style={{
          padding: '8px 12px', borderRadius: '6px',
          background: 'rgba(244,67,54,0.1)', color: '#f44336', fontSize: '12px',
        }}>
          {error}
        </div>
      )}

      <div style={{ display: 'flex', gap: '8px' }}>
        <input
          value={input}
          onChange={e => setInput(e.target.value)}
          onKeyDown={e => e.key === 'Enter' && send()}
          placeholder="输入你的问题，如：000001 后市如何？"
          disabled={loading}
          style={{
            flex: 1, padding: '12px 16px', borderRadius: '8px',
            border: '1px solid var(--border-color, #3a3a4e)',
            background: 'var(--input-bg, #1e1e2e)', color: 'var(--text-primary, #e0e0e8)',
            fontSize: '14px', outline: 'none',
          }}
        />
        <button
          className="ai-tab-btn active"
          onClick={send}
          disabled={loading || !input.trim()}
        >
          发送
        </button>
      </div>
    </div>
  );
}
