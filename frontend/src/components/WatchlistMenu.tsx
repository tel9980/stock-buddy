import { useEffect, useRef, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useStockStore } from '../stores/stockStore';

function pctClass(pct: number): string {
  return pct > 0 ? 'up' : pct < 0 ? 'down' : 'flat';
}

export default function WatchlistMenu() {
  const navigate = useNavigate();
  const { watchlist, watchQuotes, fetchWatchlist, fetchWatchQuotes, removeStock } = useStockStore();
  const [open, setOpen] = useState(false);
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
            minWidth: '280px',
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
            自选股
          </div>

          {watchlist.length === 0 ? (
            <div style={{ padding: '16px 12px', fontSize: '0.85rem', color: 'var(--text-dim)' }}>
              暂无自选股，去「个股分析」添加
            </div>
          ) : watchQuotes.length === 0 ? (
            <div style={{ padding: '16px 12px', fontSize: '0.85rem', color: 'var(--text-dim)' }}>加载中…</div>
          ) : (
            <div>
              {watchQuotes.map((q) => (
                <div
                  key={q.code}
                  className="watchlist-row"
                  onClick={() => {
                    navigate(`/stock?code=${q.code}`);
                    setOpen(false);
                  }}
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: '10px',
                    padding: '9px 12px',
                    cursor: 'pointer',
                    borderBottom: '1px solid var(--border)',
                  }}
                >
                  <div style={{ minWidth: '56px', fontWeight: 600 }}>{q.name}</div>
                  <div style={{ minWidth: '44px', color: 'var(--text-dim)', fontSize: '0.78rem' }}>{q.code}</div>
                  <div style={{ flex: 1, textAlign: 'right', fontSize: '0.9rem' }}>{q.price.toFixed(2)}</div>
                  <div className={pctClass(q.pct_change)} style={{ minWidth: '64px', textAlign: 'right', fontWeight: 600, fontSize: '0.9rem' }}>
                    {q.pct_change > 0 ? '+' : ''}
                    {q.pct_change.toFixed(2)}%
                  </div>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      removeStock(q.code);
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
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
