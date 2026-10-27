import { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useStockStore } from '../stores/stockStore';
import { api, SectorStock } from '../api';

function formatNum(n: number): string {
  if (n >= 1e8) return (n / 1e8).toFixed(2) + '亿';
  if (n >= 1e4) return (n / 1e4).toFixed(2) + '万';
  return n.toFixed(2);
}

/** 成交额：以「元」为单位进来，展示成亿/万 */
function fmtAmount(yuan: number): string {
  if (!yuan || yuan <= 0) return '—';
  if (yuan >= 1e12) return (yuan / 1e12).toFixed(2) + ' 万亿';
  if (yuan >= 1e8) return (yuan / 1e8).toFixed(0) + ' 亿';
  if (yuan >= 1e4) return (yuan / 1e4).toFixed(0) + ' 万';
  return yuan.toFixed(0);
}

/** 家数：0 通常意味着数据源没拿到，别显示成真的「0 家」 */
function fmtCount(n: number | undefined): string {
  return n && n > 0 ? String(n) : '—';
}

function pctClass(pct: number): string {
  return pct > 0 ? 'up' : pct < 0 ? 'down' : 'flat';
}

function pctStr(pct: number): string {
  const sign = pct > 0 ? '+' : '';
  return `${sign}${pct.toFixed(2)}%`;
}

export default function Dashboard() {
  const navigate = useNavigate();
  const {
    marketOverview,
    loadingMarket,
    fetchMarketOverview,
    sectorRanks,
    loadingSectors,
    fetchSectorRanks,
  } = useStockStore();

  const [expandedSector, setExpandedSector] = useState<string | null>(null);
  const [sectorStocks, setSectorStocks] = useState<SectorStock[]>([]);
  const [loadingStocks, setLoadingStocks] = useState(false);

  useEffect(() => {
    fetchMarketOverview();
    fetchSectorRanks('industry');
  }, [fetchMarketOverview, fetchSectorRanks]);

  const handleSectorClick = async (code: string) => {
    if (expandedSector === code) {
      setExpandedSector(null);
      return;
    }
    setExpandedSector(code);
    setLoadingStocks(true);
    try {
      const resp = await api.getSectorStocks(code, 1, 50);
      setSectorStocks(resp.stocks);
    } catch (e) {
      console.error('Failed to fetch sector stocks:', e);
      setSectorStocks([]);
    } finally {
      setLoadingStocks(false);
    }
  };

  const handleStockClick = (code: string) => {
    navigate(`/stock?code=${code}`);
  };

  return (
    <div>
      <h2 style={{ marginBottom: '16px' }}>大盘总览</h2>

      {/* 指数面板 */}
      {loadingMarket ? (
        <div className="chart-loading">加载中...</div>
      ) : marketOverview ? (
        <>
          <div className="index-grid">
            {marketOverview.indices.map((idx) => (
              <div key={idx.code} className="index-card">
                <div className="name">{idx.name}</div>
                <div className={`price ${pctClass(idx.pct_change)}`}>
                  {idx.price.toFixed(2)}
                </div>
                <div className={`pct ${pctClass(idx.pct_change)}`}>
                  {pctStr(idx.pct_change)} ({idx.change > 0 ? '+' : ''}{idx.change.toFixed(2)})
                </div>
                <div style={{ color: '#9090a8', fontSize: '0.78rem', marginTop: '2px' }}>
                  成交 {fmtAmount(idx.amount)}
                </div>
              </div>
            ))}
          </div>

          {/* 市场概况 */}
          <div className="card">
            <div className="card-title">市场概况</div>
            <div style={{ display: 'flex', gap: '32px', flexWrap: 'wrap' }}>
              <div>
                <span className="up" style={{ fontSize: '1.4rem', fontWeight: 700 }}>
                  {fmtCount(marketOverview.up_count)}
                </span>
                <span style={{ color: '#9090a8', marginLeft: '4px' }}>上涨</span>
              </div>
              <div>
                <span className="down" style={{ fontSize: '1.4rem', fontWeight: 700 }}>
                  {fmtCount(marketOverview.down_count)}
                </span>
                <span style={{ color: '#9090a8', marginLeft: '4px' }}>下跌</span>
              </div>
              <div>
                <span className="flat" style={{ fontSize: '1.4rem', fontWeight: 700 }}>
                  {fmtCount(marketOverview.flat_count)}
                </span>
                <span style={{ color: '#9090a8', marginLeft: '4px' }}>平盘</span>
              </div>
              <div>
                <span className="up" style={{ fontSize: '1.4rem', fontWeight: 700 }}>
                  {fmtCount(marketOverview.limit_up_count)}
                </span>
                <span style={{ color: '#9090a8', marginLeft: '4px' }}>涨停</span>
              </div>
              <div>
                <span className="down" style={{ fontSize: '1.4rem', fontWeight: 700 }}>
                  {fmtCount(marketOverview.limit_down_count)}
                </span>
                <span style={{ color: '#9090a8', marginLeft: '4px' }}>跌停</span>
              </div>
              <div>
                <span style={{ fontSize: '1.4rem', fontWeight: 700, color: '#e8804e' }}>
                  {fmtAmount(marketOverview.total_amount)}
                </span>
                <span style={{ color: '#9090a8', marginLeft: '4px' }}>两市成交额</span>
              </div>
            </div>
          </div>
        </>
      ) : (
        <div className="chart-loading">暂无数据</div>
      )}

      {/* 板块排行 */}
      <div className="card" style={{ marginTop: '16px' }}>
        <div className="card-title">
          行业板块排行
          <span style={{ fontSize: '0.8rem', color: '#9090a8', fontWeight: 400, marginLeft: '8px' }}>
            点击板块查看成分股
          </span>
        </div>
        {loadingSectors ? (
          <div className="chart-loading">加载中...</div>
        ) : (
          <table className="data-table">
            <thead>
              <tr>
                <th>板块名称</th>
                <th>涨跌幅</th>
                <th>领涨股</th>
                <th>领涨幅</th>
                <th>上涨家数</th>
                <th>下跌家数</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {sectorRanks.slice(0, 20).map((s) => (
                <>
                  <tr
                    key={s.code}
                    onClick={() => handleSectorClick(s.code)}
                    style={{ cursor: 'pointer', transition: 'background 0.15s' }}
                    onMouseEnter={(e) => e.currentTarget.style.background = 'rgba(232, 128, 78, 0.08)'}
                    onMouseLeave={(e) => e.currentTarget.style.background = ''}
                  >
                    <td style={{ fontWeight: 600 }}>{s.name}</td>
                    <td className={pctClass(s.change_pct)}>{pctStr(s.change_pct)}</td>
                    <td>{s.leader_stock || '-'}</td>
                    <td className={s.leader_pct ? pctClass(s.leader_pct) : ''}>
                      {s.leader_pct ? pctStr(s.leader_pct) : '-'}
                    </td>
                    <td className="up">{s.up_count ? s.up_count : '—'}</td>
                    <td className="down">{s.down_count ? s.down_count : '—'}</td>
                    <td style={{ color: '#e8804e', fontSize: '0.85rem' }}>
                      {expandedSector === s.code ? '收起 ▲' : '查看成分股 ▼'}
                    </td>
                  </tr>
                  {expandedSector === s.code && (
                    <tr key={s.code + '-stocks'}>
                      <td colSpan={7} style={{ padding: '12px 16px', background: 'rgba(26,26,46,0.5)' }}>
                        {loadingStocks ? (
                          <div className="chart-loading">加载成分股中...</div>
                        ) : sectorStocks.length > 0 ? (
                          <div>
                            <div style={{ marginBottom: '8px', color: '#9090a8', fontSize: '0.85rem' }}>
                              {s.name} 成分股（{sectorStocks.length} 只）
                            </div>
                            <div style={{ overflowX: 'auto' }}>
                              <table className="data-table" style={{ fontSize: '0.85rem' }}>
                                <thead>
                                  <tr>
                                    <th>代码</th>
                                    <th>名称</th>
                                    <th>最新价</th>
                                    <th>涨跌幅</th>
                                    <th>涨跌额</th>
                                    <th>成交额</th>
                                    <th>换手率</th>
                                    <th></th>
                                  </tr>
                                </thead>
                                <tbody>
                                  {sectorStocks.map((st) => (
                                    <tr key={st.code}>
                                      <td style={{ color: '#9090a8' }}>{st.code}</td>
                                      <td style={{ fontWeight: 500 }}>{st.name}</td>
                                      <td className={pctClass(st.pct_change)}>{st.price.toFixed(2)}</td>
                                      <td className={pctClass(st.pct_change)}>{pctStr(st.pct_change)}</td>
                                      <td className={pctClass(st.change)}>
                                        {st.change > 0 ? '+' : ''}{st.change.toFixed(2)}
                                      </td>
                                      <td>{formatNum(st.amount)}</td>
                                      <td>{st.turnover > 0 ? st.turnover.toFixed(2) + '%' : '-'}</td>
                                      <td>
                                        <button
                                          onClick={(e) => {
                                            e.stopPropagation();
                                            handleStockClick(st.code);
                                          }}
                                          style={{
                                            background: 'rgba(232, 128, 78, 0.2)',
                                            border: '1px solid rgba(232, 128, 78, 0.4)',
                                            color: '#e8804e',
                                            padding: '3px 10px',
                                            borderRadius: '4px',
                                            cursor: 'pointer',
                                            fontSize: '0.8rem',
                                            whiteSpace: 'nowrap',
                                          }}
                                        >
                                          分析 →
                                        </button>
                                      </td>
                                    </tr>
                                  ))}
                                </tbody>
                              </table>
                            </div>
                          </div>
                        ) : (
                          <div className="chart-loading">暂无成分股数据</div>
                        )}
                      </td>
                    </tr>
                  )}
                </>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
