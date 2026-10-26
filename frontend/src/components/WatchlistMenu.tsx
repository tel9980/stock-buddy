import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { InputNumber, Button } from 'antd';
import { useStockStore } from '../stores/stockStore';
import { api } from '../api';

function pctClass(pct: number): string {
  return pct > 0 ? 'up' : pct < 0 ? 'down' : 'flat';
}

export default function WatchlistMenu() {
  const navigate = useNavigate();
  const { watchlist, watchQuotes, fetchWatchlist, fetchWatchQuotes, removeStock } = useStockStore();
  const [open, setOpen] = useState(false);
  const [editCode, setEditCode] = useState<string | null>(null);
  const [editCost, setEditCost] = useState<number>(0);
  const [editShares, setEditShares] = useState<number>(0);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    fetchWatchlist();
  }, [fetchWatchlist]);

  useEffect(() => {
    fetchWatchQuotes();
  }, [fetchWatchQuotes, watchlist]);

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener('mousedown', handler);
    return () => document.removeEventListener('mousedown', handler);
  }, []);

  const merged = watchlist.map((item) => ({
    item,
    quote: watchQuotes.find((q) => q.code === item.stock_code),
  }));

  const openEdit = (code: string, cost: number, shares: number) => {
    setEditCode(code);
    setEditCost(cost);
    setEditShares(shares);
  };

  const saveEdit = async (code: string) => {
    try {
      const resp = await api.updateWatchlist(code, editCost, editShares);
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
      await fetchWatchlist();
    } catch (e) {
      console.error('更新自选股失败', e);
      // 写接口失败必须让用户可见，不能静默吞掉
      alert('更新自选股失败，请稍后重试');
    }
    setEditCode(null);
  };

  return (
    <div className="watchlist-menu" style={{ position: 'relative', marginLeft: 'auto' }} ref={ref}>
      <button
        className="watchlist-toggle"
        onClick={() => setOpen(!open)}
        style={{
          background: 'none',
          border: '1px solid var(--border)',
          color: 'var(--text-dim)',
          padding: '6px 14px',
          borderRadius: '6px',
          cursor: 'pointer',
          fontSize: '0.85rem',
        }}
      >
        自选股{watchlist.length > 0 && <span style={{ color: '#4dabf7', marginLeft: 4 }}>{watchlist.length}</span>}
      </button>

      {open && (
        <div
          className="watchlist-panel"
          style={{
            position: 'absolute',
            right: 0,
            top: 'calc(100% + 6px)',
            minWidth: '340px',
            maxHeight: '480px',
            overflowY: 'auto',
            background: 'var(--bg-surface)',
            border: '1px solid var(--border)',
            borderRadius: '8px',
            boxShadow: '0 8px 24px rgba(0,0,0,0.4)',
            zIndex: 100,
          }}
        >
          <div
            style={{
              padding: '8px 12px',
              fontSize: '0.8rem',
              color: 'var(--text-dim)',
              borderBottom: '1px solid var(--border)',
            }}
          >
            自选股（可编辑成本价/持股数，供做T工具联动）
          </div>

          {merged.length === 0 ? (
            <div style={{ padding: '16px 12px', fontSize: '0.85rem', color: 'var(--text-dim)' }}>
              暂无自选股，去「个股分析」添加
            </div>
          ) : (
            <div>
              {merged.map(({ item, quote }) => (
                <div
                  key={item.stock_code}
                  style={{
                    padding: '9px 12px',
                    borderBottom: '1px solid var(--border)',
                  }}
                >
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '10px',
                      cursor: 'pointer',
                    }}
                    onClick={() => {
                      navigate(`/stock?code=${item.stock_code}`);
                      setOpen(false);
                    }}
                  >
                    <div style={{ minWidth: '56px', fontWeight: 600 }}>{quote?.name ?? item.stock_code}</div>
                    <div style={{ minWidth: '44px', color: 'var(--text-dim)', fontSize: '0.78rem' }}>{item.stock_code}</div>
                    <div style={{ flex: 1, textAlign: 'right', fontSize: '0.9rem' }}>{quote ? quote.price.toFixed(2) : '-'}</div>
                    <div className={pctClass(quote?.pct_change ?? 0)} style={{ minWidth: '64px', textAlign: 'right', fontWeight: 600, fontSize: '0.9rem' }}>
                      {quote ? (quote.pct_change > 0 ? '+' : '') + quote.pct_change.toFixed(2) + '%' : ''}
                    </div>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        openEdit(item.stock_code, item.cost_price, item.shares);
                      }}
                      style={{
                        background: 'none',
                        border: '1px solid var(--border)',
                        color: 'var(--text-dim)',
                        borderRadius: '4px',
                        padding: '1px 7px',
                        fontSize: '0.75rem',
                        cursor: 'pointer',
                      }}
                    >
                      编辑
                    </button>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        removeStock(item.stock_code);
                      }}
                      style={{
                        background: 'none',
                        border: '1px solid var(--border)',
                        color: 'var(--text-dim)',
                        borderRadius: '4px',
                        padding: '1px 7px',
                        fontSize: '0.75rem',
                        cursor: 'pointer',
                      }}
                    >
                      删除
                    </button>
                  </div>

                  {editCode === item.stock_code && (
                    <div
                      style={{ display: 'flex', gap: 8, alignItems: 'center', marginTop: 8 }}
                      onClick={(e) => e.stopPropagation()}
                    >
                      <span style={{ fontSize: '0.78rem', color: 'var(--text-dim)' }}>成本价</span>
                      <InputNumber
                        size="small"
                        min={0}
                        step={0.01}
                        value={editCost}
                        onChange={(v) => setEditCost(v ?? 0)}
                        style={{ width: 90 }}
                      />
                      <span style={{ fontSize: '0.78rem', color: 'var(--text-dim)' }}>股数</span>
                      <InputNumber
                        size="small"
                        min={0}
                        step={100}
                        value={editShares}
                        onChange={(v) => setEditShares(v ?? 0)}
                        style={{ width: 90 }}
                      />
                      <Button size="small" type="primary" onClick={() => saveEdit(item.stock_code)}>
                        保存
                      </Button>
                      <Button size="small" onClick={() => setEditCode(null)}>
                        取消
                      </Button>
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
