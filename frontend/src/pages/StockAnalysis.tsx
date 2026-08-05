import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useStockStore } from '../stores/stockStore';
import { api, Kline, SignalAnalysis, TradeSignal } from '../api';
import KLineChart from '../components/KLineChart';

function pctClass(pct: number): string {
  return pct > 0 ? 'up' : pct < 0 ? 'down' : 'flat';
}

function formatVolume(v: number): string {
  if (v >= 1e8) return (v / 1e8).toFixed(2) + '亿';
  if (v >= 1e4) return (v / 1e4).toFixed(2) + '万';
  return v.toFixed(0);
}

function strengthClass(strength: string): string {
  if (strength.includes('强')) return 'signal-strong';
  if (strength.includes('中')) return 'signal-medium';
  return 'signal-weak';
}

function actionColor(action: string): string {
  if (action.includes('强烈买入')) return '#ef4444';
  if (action.includes('买入')) return '#ff6b6b';
  if (action.includes('强烈卖出')) return '#22c55e';
  if (action.includes('卖出') || action.includes('减仓')) return '#51cf66';
  return '#9090a8';
}

function actionBg(score: number): string {
  if (score >= 4) return 'rgba(239,68,68,0.15)';
  if (score >= 1) return 'rgba(239,68,68,0.08)';
  if (score <= -4) return 'rgba(34,197,94,0.15)';
  if (score <= -1) return 'rgba(34,197,94,0.08)';
  return 'rgba(144,144,168,0.08)';
}

export default function StockAnalysis() {
  const [searchParams, setSearchParams] = useSearchParams();
  const {
    searchCode,
    setSearchCode,
    quotes,
    loadingQuotes,
    fetchQuotes,
    watchlist,
    addStock,
    removeStock,
  } = useStockStore();

  const [klines, setKlines] = useState<Kline[]>([]);
  const [loadingKline, setLoadingKline] = useState(false);
  const [period, setPeriod] = useState('daily');
  const [signalAnalysis, setSignalAnalysis] = useState<SignalAnalysis | null>(null);
  const [loadingSignals, setLoadingSignals] = useState(false);
  const [showAllSignals, setShowAllSignals] = useState(false);

  // 从 URL 参数读取股票代码
  useEffect(() => {
    const code = searchParams.get('code');
    if (code && code !== searchCode) {
      setSearchCode(code);
      fetchQuotes([code]);
      fetchKline(code, period);
      fetchSignals(code, period);
    } else if (!searchParams.get('code') && searchCode) {
      // 没有URL参数但有searchCode时也加载数据
      fetchQuotes([searchCode]);
      fetchKline(searchCode, period);
      fetchSignals(searchCode, period);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams]);

  // 搜索股票
  const handleSearch = () => {
    if (!searchCode) return;
    setSearchParams({ code: searchCode });
    fetchQuotes([searchCode]);
    fetchKline(searchCode, period);
    fetchSignals(searchCode, period);
  };

  // 加载K线
  const fetchKline = async (code: string, p: string) => {
    setLoadingKline(true);
    try {
      const data = await api.getKline(code, p, 365);
      setKlines(data);
    } catch (e) {
      console.error('Failed to fetch kline:', e);
      setKlines([]);
    } finally {
      setLoadingKline(false);
    }
  };

  // 加载买卖信号
  const fetchSignals = async (code: string, p: string) => {
    setLoadingSignals(true);
    try {
      const data = await api.getTradeSignals(code, p, 120);
      setSignalAnalysis(data);
    } catch (e) {
      console.error('Failed to fetch signals:', e);
      setSignalAnalysis(null);
    } finally {
      setLoadingSignals(false);
    }
  };

  // 切换周期
  const handlePeriodChange = (p: string) => {
    setPeriod(p);
    if (searchCode) {
      fetchKline(searchCode, p);
      fetchSignals(searchCode, p);
    }
  };

  // 回车搜索
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') handleSearch();
  };

  const currentQuote = quotes[0];
  const isWatched = watchlist.includes(searchCode);

  // K线图上显示的信号（只显示最近的可视范围）
  const chartSignals: TradeSignal[] = signalAnalysis?.signals.slice(-20) || [];

  // 当前信号列表
  const displaySignals = showAllSignals
    ? (signalAnalysis?.signals || [])
    : (signalAnalysis?.current_signals || []);

  return (
    <div>
      <h2 style={{ marginBottom: '16px' }}>个股分析</h2>

      {/* 搜索栏 */}
      <div className="input-group">
        <input
          type="text"
          value={searchCode}
          onChange={(e) => setSearchCode(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="输入股票代码，如 000001, 600000"
        />
        <button onClick={handleSearch}>查询</button>
        {isWatched ? (
          <button
            onClick={() => removeStock(searchCode)}
            style={{ background: '#22223a' }}
          >
            取消自选
          </button>
        ) : (
          <button
            onClick={() => addStock(searchCode)}
            style={{ background: '#22223a' }}
          >
            加入自选
          </button>
        )}
      </div>

      {/* 实时行情 */}
      {loadingQuotes ? (
        <div className="chart-loading">加载行情中...</div>
      ) : currentQuote ? (
        <div className="card">
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '16px' }}>
            <div>
              <span style={{ fontSize: '1.3rem', fontWeight: 700 }}>{currentQuote.name}</span>
              <span style={{ color: '#9090a8', marginLeft: '8px' }}>{currentQuote.code}</span>
            </div>
            <div className={pctClass(currentQuote.pct_change)} style={{ fontSize: '2rem', fontWeight: 700 }}>
              {currentQuote.price.toFixed(2)}
            </div>
          </div>
          <div className="quote-panel">
            <div className="quote-item">
              <div className="name">涨跌幅</div>
              <div className={`price ${pctClass(currentQuote.pct_change)}`}>
                {currentQuote.pct_change > 0 ? '+' : ''}{currentQuote.pct_change.toFixed(2)}%
              </div>
            </div>
            <div className="quote-item">
              <div className="name">涨跌额</div>
              <div className={`price ${pctClass(currentQuote.change)}`}>
                {currentQuote.change > 0 ? '+' : ''}{currentQuote.change.toFixed(2)}
              </div>
            </div>
            <div className="quote-item">
              <div className="name">今开</div>
              <div className="price">{currentQuote.open.toFixed(2)}</div>
            </div>
            <div className="quote-item">
              <div className="name">昨收</div>
              <div className="price">{currentQuote.pre_close.toFixed(2)}</div>
            </div>
            <div className="quote-item">
              <div className="name">最高</div>
              <div className="price up">{currentQuote.high.toFixed(2)}</div>
            </div>
            <div className="quote-item">
              <div className="name">最低</div>
              <div className="price down">{currentQuote.low.toFixed(2)}</div>
            </div>
            <div className="quote-item">
              <div className="name">成交量</div>
              <div className="price">{formatVolume(currentQuote.volume)}</div>
            </div>
            <div className="quote-item">
              <div className="name">成交额</div>
              <div className="price">{formatVolume(currentQuote.amount)}</div>
            </div>
          </div>
        </div>
      ) : (
        <div className="chart-loading">输入股票代码查询行情</div>
      )}

      {/* 买卖信号综合评估 */}
      {signalAnalysis && (
        <div
          className="card"
          style={{
            marginTop: '16px',
            background: actionBg(signalAnalysis.overall_score),
            borderColor: actionColor(signalAnalysis.overall_action),
          }}
        >
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <div>
              <div className="card-title" style={{ marginBottom: '4px' }}>买卖点综合评估</div>
              <div style={{ fontSize: '0.85rem', color: '#9090a8' }}>
                基于 MACD / KDJ / RSI / 布林带 / 量价 / 支撑压力位 多维度信号
              </div>
            </div>
            <div style={{ textAlign: 'right' }}>
              <div
                style={{
                  fontSize: '1.8rem',
                  fontWeight: 700,
                  color: actionColor(signalAnalysis.overall_action),
                }}
              >
                {signalAnalysis.overall_action}
              </div>
              <div style={{ fontSize: '0.85rem', color: '#9090a8' }}>
                综合评分: <span style={{ color: actionColor(signalAnalysis.overall_action), fontWeight: 600 }}>
                  {signalAnalysis.overall_score > 0 ? '+' : ''}{signalAnalysis.overall_score}
                </span>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* K线图（带买卖标记）*/}
      <div className="kline-container">
        <div style={{ display: 'flex', gap: '4px', marginBottom: '12px', alignItems: 'center' }}>
          {[
            { label: '日K', value: 'daily' },
            { label: '周K', value: 'weekly' },
            { label: '月K', value: 'monthly' },
            { label: '5分钟', value: '5' },
            { label: '15分钟', value: '15' },
            { label: '60分钟', value: '60' },
          ].map((p) => (
            <button
              key={p.value}
              onClick={() => handlePeriodChange(p.value)}
              style={{
                background: period === p.value ? 'var(--primary)' : 'var(--bg-surface-light)',
                border: 'none',
                color: period === p.value ? '#fff' : 'var(--text-dim)',
                padding: '4px 12px',
                borderRadius: '4px',
                cursor: 'pointer',
                fontSize: '0.82rem',
              }}
            >
              {p.label}
            </button>
          ))}
          {chartSignals.length > 0 && (
            <div style={{ marginLeft: 'auto', display: 'flex', gap: '12px', fontSize: '0.8rem' }}>
              <span>
                <span style={{ display: 'inline-block', width: '10px', height: '10px', background: '#ef4444', borderRadius: '50%', marginRight: '4px' }}></span>
                买点 ({chartSignals.filter(s => s.signal_type === 'Buy').length})
              </span>
              <span>
                <span style={{ display: 'inline-block', width: '10px', height: '10px', background: '#22c55e', borderRadius: '50%', marginRight: '4px' }}></span>
                卖点 ({chartSignals.filter(s => s.signal_type === 'Sell').length})
              </span>
            </div>
          )}
        </div>
        {loadingKline ? (
          <div className="chart-loading">加载K线数据中...</div>
        ) : (
          <KLineChart klines={klines} signals={chartSignals} height={500} />
        )}
      </div>

      {/* 信号列表 */}
      {loadingSignals ? (
        <div className="card" style={{ marginTop: '16px' }}>
          <div className="chart-loading">分析买卖信号中...</div>
        </div>
      ) : signalAnalysis && displaySignals.length > 0 ? (
        <div className="card" style={{ marginTop: '16px' }}>
          <div className="card-title" style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <span>
              {showAllSignals ? '全部买卖信号' : '当前活跃信号'}
              <span style={{ fontSize: '0.8rem', color: '#9090a8', fontWeight: 400, marginLeft: '8px' }}>
                ({displaySignals.length} 条)
              </span>
            </span>
            <button
              onClick={() => setShowAllSignals(!showAllSignals)}
              style={{
                background: 'var(--bg-surface-light)',
                border: '1px solid var(--border)',
                color: 'var(--text-dim)',
                padding: '4px 12px',
                borderRadius: '4px',
                cursor: 'pointer',
                fontSize: '0.82rem',
              }}
            >
              {showAllSignals ? '查看当前信号' : '查看全部信号'}
            </button>
          </div>
          <div style={{ overflowX: 'auto' }}>
            <table className="data-table">
              <thead>
                <tr>
                  <th>类型</th>
                  <th>日期</th>
                  <th>价格</th>
                  <th>来源</th>
                  <th>强度</th>
                  <th>说明</th>
                </tr>
              </thead>
              <tbody>
                {displaySignals.map((sig, i) => (
                  <tr key={i}>
                    <td>
                      <span
                        style={{
                          display: 'inline-block',
                          padding: '2px 8px',
                          borderRadius: '4px',
                          fontSize: '0.8rem',
                          fontWeight: 600,
                          background: sig.signal_type === 'Buy' ? 'rgba(239,68,68,0.2)' : 'rgba(34,197,94,0.2)',
                          color: sig.signal_type === 'Buy' ? '#ef4444' : '#22c55e',
                        }}
                      >
                        {sig.signal_type === 'Buy' ? '买入' : '卖出'}
                      </span>
                    </td>
                    <td>{sig.date}</td>
                    <td className={sig.signal_type === 'Buy' ? 'up' : 'down'}>{sig.price.toFixed(2)}</td>
                    <td style={{ color: '#e8804e' }}>{sig.source}</td>
                    <td>
                      <span className={strengthClass(sig.strength)} style={{
                        padding: '2px 6px',
                        borderRadius: '3px',
                        fontSize: '0.8rem',
                      }}>
                        {sig.strength}
                      </span>
                    </td>
                    <td style={{ color: 'var(--text-dim)', fontSize: '0.85rem' }}>{sig.description}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ) : null}

      {/* 支撑压力位 */}
      {signalAnalysis && (
        <div className="card" style={{ marginTop: '16px' }}>
          <div className="card-title">支撑位 / 压力位</div>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '16px' }}>
            <div>
              <div style={{ color: '#ef4444', fontSize: '0.9rem', marginBottom: '8px', fontWeight: 600 }}>支撑位</div>
              {signalAnalysis.support_resistance.supports.length > 0 ? (
                signalAnalysis.support_resistance.supports.map((s, i) => (
                  <div key={i} style={{ display: 'flex', justifyContent: 'space-between', padding: '4px 0' }}>
                    <span style={{ color: 'var(--text-dim)' }}>{s.source} ({s.strength})</span>
                    <span className="up" style={{ fontWeight: 600 }}>{s.price.toFixed(2)}</span>
                  </div>
                ))
              ) : (
                <div style={{ color: '#9090a8' }}>暂无</div>
              )}
            </div>
            <div>
              <div style={{ color: '#22c55e', fontSize: '0.9rem', marginBottom: '8px', fontWeight: 600 }}>压力位</div>
              {signalAnalysis.support_resistance.resistances.length > 0 ? (
                signalAnalysis.support_resistance.resistances.map((r, i) => (
                  <div key={i} style={{ display: 'flex', justifyContent: 'space-between', padding: '4px 0' }}>
                    <span style={{ color: 'var(--text-dim)' }}>{r.source} ({r.strength})</span>
                    <span className="down" style={{ fontWeight: 600 }}>{r.price.toFixed(2)}</span>
                  </div>
                ))
              ) : (
                <div style={{ color: '#9090a8' }}>暂无</div>
              )}
            </div>
          </div>
          <div style={{ marginTop: '12px', padding: '8px 12px', background: 'rgba(232,128,78,0.08)', borderRadius: '6px', display: 'flex', justifyContent: 'space-between' }}>
            <span style={{ color: '#9090a8' }}>当前价格</span>
            <span style={{ fontWeight: 700, color: '#e8804e' }}>{signalAnalysis.support_resistance.current_price.toFixed(2)}</span>
          </div>
        </div>
      )}
    </div>
  );
}
