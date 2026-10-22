import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { useStockStore } from '../stores/stockStore';
import { usePersistedState } from '../stores/persistedState';
import { api, Kline, SignalAnalysis, TradeSignal, VolumePriceAnalysis, VpRelationship, TapeReadingAnalysis } from '../api';
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

// 量价关系的中文标签与配色
const VP_RELATION_META: Record<VpRelationship, { label: string; color: string; bg: string }> = {
  VolumeUpPriceUp: { label: '量增价升', color: '#ef4444', bg: 'rgba(239,68,68,0.18)' },
  VolumeUpPriceDown: { label: '量增价跌', color: '#22c55e', bg: 'rgba(34,197,94,0.18)' },
  VolumeFlatPriceUp: { label: '价升量平', color: '#ff6b6b', bg: 'rgba(255,107,107,0.14)' },
  VolumeFlatPriceDown: { label: '价跌量平', color: '#51cf66', bg: 'rgba(81,207,102,0.14)' },
  VolumeDownPriceUp: { label: '量缩价升', color: '#ffa94d', bg: 'rgba(255,169,77,0.16)' },
  VolumeDownPriceDown: { label: '量缩价跌', color: '#74c0fc', bg: 'rgba(116,192,252,0.16)' },
  Neutral: { label: '价平量平', color: '#9090a8', bg: 'rgba(144,144,168,0.12)' },
};

function verdictColor(v: string): string {
  if (v.includes('看多')) return '#ef4444';
  if (v.includes('看空')) return '#22c55e';
  return '#9090a8';
}

// 盘口解读 verdict 配色（看多红、看空绿、谨慎/关注橙、中性灰）
function tapeVerdictColor(v: string): string {
  if (v.includes('机构主导') || v.includes('机构吸筹')) return '#ef4444';
  if (v.includes('主力出货') || v.includes('对倒诱多')) return '#22c55e';
  if (v.includes('散户推动') || v.includes('游资主导')) return '#ffa94d';
  return '#9090a8';
}

function tapeCategoryColor(c: string): string {
  if (c === '偏多') return '#ef4444';
  if (c === '偏空') return '#22c55e';
  if (c === '关注') return '#ffa94d';
  return '#9090a8';
}

function vpSignalColor(t: string): string {
  if (t === 'Buy') return '#ef4444';
  if (t === 'Sell') return '#22c55e';
  return '#ffa94d';
}

function vpSignalLabel(t: string): string {
  if (t === 'Buy') return '偏多';
  if (t === 'Sell') return '偏空';
  return '关注';
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
  const [period, setPeriod] = usePersistedState('stock.period', 'daily');
  const [signalAnalysis, setSignalAnalysis] = useState<SignalAnalysis | null>(null);
  const [loadingSignals, setLoadingSignals] = useState(false);
  const [showAllSignals, setShowAllSignals] = usePersistedState('stock.showAllSignals', false);
  const [vpAnalysis, setVpAnalysis] = useState<VolumePriceAnalysis | null>(null);
  const [loadingVp, setLoadingVp] = useState(false);
  const [tapeReading, setTapeReading] = useState<TapeReadingAnalysis | null>(null);
  const [loadingTape, setLoadingTape] = useState(false);

  // 从 URL 参数读取股票代码
  useEffect(() => {
    const code = searchParams.get('code');
    if (code && code !== searchCode) {
      setSearchCode(code);
      fetchQuotes([code]);
      fetchKline(code, period);
      fetchSignals(code, period);
      fetchVp(code, period);
      fetchTape(code, period);
    } else if (!searchParams.get('code') && searchCode) {
      // 没有URL参数但有searchCode时也加载数据
      fetchQuotes([searchCode]);
      fetchKline(searchCode, period);
      fetchSignals(searchCode, period);
      fetchVp(searchCode, period);
      fetchTape(searchCode, period);
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
    fetchVp(searchCode, period);
    fetchTape(searchCode, period);
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
      fetchVp(searchCode, p);
      fetchTape(searchCode, p);
    }
  };

  // 加载量价分析
  const fetchVp = async (code: string, p: string) => {
    setLoadingVp(true);
    try {
      const data = await api.getVolumePriceAnalysis(code, p, 120);
      setVpAnalysis(data);
    } catch (e) {
      console.error('Failed to fetch volume-price analysis:', e);
      setVpAnalysis(null);
    } finally {
      setLoadingVp(false);
    }
  };

  // 加载盘口解读
  const fetchTape = async (code: string, p: string) => {
    setLoadingTape(true);
    try {
      const data = await api.getTapeReading(code, p, 120);
      setTapeReading(data);
    } catch (e) {
      console.error('Failed to fetch tape reading:', e);
      setTapeReading(null);
    } finally {
      setLoadingTape(false);
    }
  };

  // 回车搜索
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') handleSearch();
  };

  const currentQuote = quotes[0];
  const isWatched = watchlist.some((w) => w.stock_code === searchCode);

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

      {/* 量价分析 */}
      {loadingVp ? (
        <div className="card" style={{ marginTop: '16px' }}>
          <div className="chart-loading">分析量价关系中...</div>
        </div>
      ) : vpAnalysis ? (
        <div className="card" style={{ marginTop: '16px' }}>
          <div className="card-title">量价分析</div>

          {/* 概览 */}
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: '12px', marginBottom: '16px' }}>
            <div style={{ padding: '10px 16px', borderRadius: '8px', background: 'rgba(232,128,78,0.08)' }}>
              <div style={{ fontSize: '0.78rem', color: '#9090a8' }}>量价 verdict</div>
              <div style={{ fontSize: '1.3rem', fontWeight: 700, color: verdictColor(vpAnalysis.verdict) }}>
                {vpAnalysis.verdict}
              </div>
            </div>
            <div style={{ padding: '10px 16px', borderRadius: '8px', background: 'rgba(144,144,168,0.08)' }}>
              <div style={{ fontSize: '0.78rem', color: '#9090a8' }}>量能状态</div>
              <div style={{ fontSize: '1.3rem', fontWeight: 700, color: '#e8804e' }}>
                {vpAnalysis.volume_status}
              </div>
            </div>
            <div style={{ padding: '10px 16px', borderRadius: '8px', background: 'rgba(144,144,168,0.08)' }}>
              <div style={{ fontSize: '0.78rem', color: '#9090a8' }}>当前量比 (量/MA5)</div>
              <div style={{ fontSize: '1.3rem', fontWeight: 700, color: '#e0e0e8' }}>
                {vpAnalysis.current_volume_ratio.toFixed(2)}
              </div>
            </div>
          </div>

          {/* 关键信号 */}
          {vpAnalysis.signals.length > 0 && (
            <div style={{ marginBottom: '16px' }}>
              <div style={{ fontSize: '0.85rem', color: '#9090a8', marginBottom: '8px' }}>关键量价信号</div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                {vpAnalysis.signals.map((s, i) => (
                  <div
                    key={i}
                    style={{
                      display: 'flex',
                      gap: '10px',
                      alignItems: 'flex-start',
                      padding: '8px 12px',
                      borderRadius: '6px',
                      background: 'var(--bg-surface-light)',
                      borderLeft: `3px solid ${vpSignalColor(s.signal_type)}`,
                    }}
                  >
                    <span
                      style={{
                        flexShrink: 0,
                        padding: '2px 8px',
                        borderRadius: '4px',
                        fontSize: '0.78rem',
                        fontWeight: 600,
                        color: '#fff',
                        background: vpSignalColor(s.signal_type),
                      }}
                    >
                      {vpSignalLabel(s.signal_type)}
                    </span>
                    <div>
                      <div style={{ fontWeight: 600, color: '#e0e0e8', fontSize: '0.9rem' }}>
                        {s.name}
                        <span style={{ marginLeft: '8px', fontSize: '0.75rem', color: '#9090a8' }}>
                          {s.date} · 强度 {s.strength}
                        </span>
                      </div>
                      <div style={{ fontSize: '0.82rem', color: 'var(--text-dim)', marginTop: '2px' }}>
                        {s.description}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* 近期量价关系 */}
          <div style={{ marginBottom: '16px' }}>
            <div style={{ fontSize: '0.85rem', color: '#9090a8', marginBottom: '8px' }}>近期量价关系</div>
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: '6px' }}>
              {vpAnalysis.recent_bars.slice(-14).map((b, i) => {
                const meta = VP_RELATION_META[b.relationship];
                return (
                  <div
                    key={i}
                    title={`${b.date} 收 ${b.close.toFixed(2)} (${b.pct_change > 0 ? '+' : ''}${b.pct_change.toFixed(2)}%) 量比 ${b.volume_ratio.toFixed(2)}`}
                    style={{
                      padding: '4px 8px',
                      borderRadius: '4px',
                      fontSize: '0.78rem',
                      color: meta.color,
                      background: meta.bg,
                      whiteSpace: 'nowrap',
                    }}
                  >
                    {meta.label}
                  </div>
                );
              })}
            </div>
            <div style={{ fontSize: '0.75rem', color: '#9090a8', marginTop: '6px' }}>
              左→右为时间由远及近；标签颜色：红=偏多，绿=偏空，橙/蓝=需关注
            </div>
          </div>

          {/* 结论 */}
          <div
            style={{
              padding: '12px 14px',
              borderRadius: '8px',
              background: 'rgba(232,128,78,0.06)',
              border: '1px solid rgba(232,128,78,0.2)',
              fontSize: '0.88rem',
              lineHeight: 1.7,
              color: 'var(--text-dim)',
            }}
          >
            {vpAnalysis.conclusion}
          </div>
        </div>
      ) : null}

      {/* 盘口解读（Tape Reading） */}
      {loadingTape ? (
        <div className="card" style={{ marginTop: '16px' }}>
          <div className="chart-loading">解读盘口中...</div>
        </div>
      ) : tapeReading ? (
        <div className="card" style={{ marginTop: '16px' }}>
          <div className="card-title">盘口解读（Tape Reading）</div>

          {/* 概览 */}
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: '12px', marginBottom: '16px' }}>
            <div style={{ padding: '10px 16px', borderRadius: '8px', background: 'rgba(232,128,78,0.08)' }}>
              <div style={{ fontSize: '0.75rem', color: '#9090a8' }}>盘口 verdict</div>
              <div style={{ fontSize: '1.3rem', fontWeight: 700, color: tapeVerdictColor(tapeReading.verdict) }}>
                {tapeReading.verdict}
              </div>
            </div>
            <div style={{ padding: '10px 16px', borderRadius: '8px', background: 'rgba(144,144,168,0.08)' }}>
              <div style={{ fontSize: '0.75rem', color: '#9090a8' }}>置信度</div>
              <div style={{ fontSize: '1.1rem', fontWeight: 700 }}>{tapeReading.confidence}</div>
            </div>
            <div style={{ padding: '10px 16px', borderRadius: '8px', background: 'rgba(144,144,168,0.08)' }}>
              <div style={{ fontSize: '0.75rem', color: '#9090a8' }}>主力净流入占比</div>
              <div style={{ fontSize: '1.1rem', fontWeight: 700, color: tapeReading.main_participation >= 0 ? '#ef4444' : '#22c55e' }}>
                {tapeReading.main_participation >= 0 ? '+' : ''}{tapeReading.main_participation.toFixed(2)}%
              </div>
            </div>
            <div style={{ padding: '10px 16px', borderRadius: '8px', background: 'rgba(144,144,168,0.08)' }}>
              <div style={{ fontSize: '0.75rem', color: '#9090a8' }}>超大单+大单净买</div>
              <div style={{ fontSize: '1.1rem', fontWeight: 700, color: tapeReading.big_order_net >= 0 ? '#ef4444' : '#22c55e' }}>
                {tapeReading.big_order_net >= 0 ? '+' : ''}{formatVolume(tapeReading.big_order_net * 10000)}
              </div>
            </div>
          </div>

          {/* 龙虎榜席位验证 */}
          {tapeReading.dragon_tiger && (
            <div style={{ marginBottom: '16px', padding: '10px 14px', borderRadius: '8px', background: 'rgba(116,192,252,0.08)', border: '1px solid rgba(116,192,252,0.2)' }}>
              <div style={{ display: 'flex', flexWrap: 'wrap', gap: '12px', fontSize: '0.85rem', color: 'var(--text-dim)' }}>
                <span>龙虎榜({tapeReading.dragon_tiger.date})：{tapeReading.dragon_tiger.reason}</span>
                <span>榜净买 <b style={{ color: tapeReading.dragon_tiger.net_amount >= 0 ? '#ef4444' : '#22c55e' }}>{formatVolume(tapeReading.dragon_tiger.net_amount * 10000)}</b></span>
                <span>机构席位净买 <b style={{ color: tapeReading.dragon_tiger.institution_net_buy >= 0 ? '#ef4444' : '#22c55e' }}>{formatVolume(tapeReading.dragon_tiger.institution_net_buy * 10000)}</b></span>
                {tapeReading.dragon_tiger.hot_money_tags.length > 0 && (
                  <span>游资标签：{tapeReading.dragon_tiger.hot_money_tags.join('、')}</span>
                )}
              </div>
            </div>
          )}

          {/* 信号列表 */}
          {tapeReading.signals.length > 0 && (
            <div style={{ marginBottom: '16px' }}>
              <div style={{ fontSize: '0.85rem', color: '#9090a8', marginBottom: '8px' }}>关键盘口信号</div>
              <div style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}>
                {tapeReading.signals.map((s, i) => (
                  <div key={i} style={{ padding: '8px 12px', borderRadius: '6px', background: 'rgba(144,144,168,0.06)' }}>
                    <div style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
                      <span style={{ fontSize: '0.7rem', padding: '1px 8px', borderRadius: '10px', color: tapeCategoryColor(s.category), border: `1px solid ${tapeCategoryColor(s.category)}` }}>{s.category}</span>
                      <span style={{ fontWeight: 600 }}>{s.name}</span>
                      <span style={{ fontSize: '0.75rem', color: '#9090a8' }}>强度 {s.strength}/5</span>
                    </div>
                    <div style={{ fontSize: '0.82rem', color: 'var(--text-dim)', marginTop: '4px', lineHeight: 1.6 }}>{s.detail}</div>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* 结论 */}
          <div
            style={{
              padding: '12px 14px',
              borderRadius: '8px',
              background: 'rgba(232,128,78,0.06)',
              border: '1px solid rgba(232,128,78,0.2)',
              fontSize: '0.88rem',
              lineHeight: 1.7,
              color: 'var(--text-dim)',
            }}
          >
            {tapeReading.conclusion}
          </div>
        </div>
      ) : null}
    </div>
  );
}
