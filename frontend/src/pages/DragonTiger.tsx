import { useState, useEffect, useRef } from 'react'
import { Card, Table, Tag, DatePicker, Spin, Empty, message } from 'antd'
import { api, DragonTigerEntry, DragonTigerOverview } from '../api'
import { usePersistedState } from '../stores/persistedState'
import dayjs from 'dayjs'

function colorNum(val: number): string {
  return val > 0 ? '#cf1322' : val < 0 ? '#3f8600' : '#666'
}

function formatAmount(val: number): string {
  if (Math.abs(val) >= 10000) return (val / 10000).toFixed(2) + '亿'
  return val.toFixed(0) + '万'
}

export default function DragonTigerPage() {
  const [overview, setOverview] = useState<DragonTigerOverview | null>(null)
  const [loading, setLoading] = useState(false)
  const [date, setDate] = usePersistedState('dragontiger.date', '')
  const userPicked = useRef(false)

  useEffect(() => { loadData() }, [])

  async function loadData(d?: string) {
    setLoading(true)
    try {
      const ov = await api.getDragonTigerOverview(d)
      setOverview(ov)
      if (!d && ov.entries.length > 0 && !userPicked.current) {
        setDate(ov.entries[0].date)
      }
    } catch {
      message.error('加载龙虎榜数据失败')
    }
    setLoading(false)
  }

  function handleDateChange(d: dayjs.Dayjs | null) {
    if (d) {
      userPicked.current = true
      const ds = d.format('YYYY-MM-DD')
      setDate(ds)
      loadData(ds)
    }
  }

  if (loading) return <Spin size="large" style={{ display: 'block', margin: '100px auto' }} />

  return (
    <div>
      <h2 style={{ margin: '0 0 16px' }}>龙虎榜</h2>

      <div style={{ display: 'flex', gap: 12, marginBottom: 16, flexWrap: 'wrap', alignItems: 'center' }}>
        <DatePicker
          value={date ? dayjs(date) : null}
          format="YYYY-MM-DD"
          onChange={handleDateChange}
          placeholder="选择日期"
          style={{ width: 160 }}
        />
      </div>

      {overview ? (
        <>
          <div style={{ display: 'flex', gap: 12, marginBottom: 16, flexWrap: 'wrap' }}>
            <Card size="small" style={{ flex: 1, minWidth: 140 }}>
              <div style={{ color: '#999', fontSize: 12 }}>上榜总数</div>
              <div style={{ fontSize: 28, fontWeight: 'bold' }}>{overview.total_count}</div>
            </Card>
            <Card size="small" style={{ flex: 1, minWidth: 140 }}>
              <div style={{ color: '#999', fontSize: 12 }}>净买入家数</div>
              <div style={{ fontSize: 28, fontWeight: 'bold', color: '#cf1322' }}>{overview.net_buy_count}</div>
            </Card>
            <Card size="small" style={{ flex: 1, minWidth: 140 }}>
              <div style={{ color: '#999', fontSize: 12 }}>净卖出家数</div>
              <div style={{ fontSize: 28, fontWeight: 'bold', color: '#3f8600' }}>{overview.net_sell_count}</div>
            </Card>
            <Card size="small" style={{ flex: 1, minWidth: 140 }}>
              <div style={{ color: '#999', fontSize: 12 }}>机构净买入</div>
              <div style={{ fontSize: 20, fontWeight: 'bold', color: colorNum(overview.institution_net_buy_total) }}>
                {formatAmount(overview.institution_net_buy_total)}
              </div>
            </Card>
          </div>

          <Table
            dataSource={overview.entries}
            rowKey="code"
            size="small"
            scroll={{ y: 600 }}
            columns={[
              { title: '代码', dataIndex: 'code', width: 80 },
              { title: '名称', dataIndex: 'name', width: 100 },
              { title: '涨跌幅', dataIndex: 'pct_change', width: 80, render: (v: number) => <span style={{ color: colorNum(v) }}>{v.toFixed(2)}%</span> },
              { title: '换手率', dataIndex: 'turnover', width: 80, render: (v: number) => `${v.toFixed(1)}%` },
              { title: '成交额', dataIndex: 'amount', width: 100, render: (v: number) => formatAmount(v) },
              { title: '净买入', dataIndex: 'net_amount', width: 100, render: (v: number) => <span style={{ color: colorNum(v) }}>{formatAmount(v)}</span> },
              { title: '买入总额', dataIndex: 'total_buy', width: 100, render: (v: number) => formatAmount(v) },
              { title: '卖出总额', dataIndex: 'total_sell', width: 100, render: (v: number) => formatAmount(v) },
              { title: '龙虎榜占比', dataIndex: 'lhb_amount_ratio', width: 90, render: (v: number) => `${v.toFixed(1)}%` },
              { title: '上榜原因', dataIndex: 'reason', width: 180, render: (v: string) => <Tag>{v}</Tag> },
            ]}
          />
        </>
      ) : (
        <Empty description="暂无龙虎榜数据" />
      )}
    </div>
  )
}
