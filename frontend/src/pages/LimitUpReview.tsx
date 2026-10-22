import { useState, useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { Card, Table, Tag, Spin, Tabs, Progress, Empty, message } from 'antd'
import { api, LimitUpReview as LimitUpReviewData, LimitUpStock, BrokenBoardStock, LimitDownStock, ConsecutiveBoardTier } from '../api'

function colorPct(val: number): string {
  return val > 0 ? '#cf1322' : val < 0 ? '#3f8600' : '#666'
}

export default function LimitUpReviewPage() {
  const navigate = useNavigate()
  const [data, setData] = useState<LimitUpReviewData | null>(null)
  const [loading, setLoading] = useState(false)

  useEffect(() => { loadData() }, [])

  async function loadData() {
    setLoading(true)
    try {
      const d = await api.getLimitUpReview()
      setData(d)
    } catch {
      message.error('加载涨停复盘数据失败')
    }
    setLoading(false)
  }

  if (loading) return <Spin size="large" style={{ display: 'block', margin: '100px auto' }} />

  return (
    <div>
      <h2 style={{ margin: '0 0 16px' }}>涨停复盘</h2>

      {data ? (
        <>
          <div style={{ display: 'flex', gap: 12, marginBottom: 16, flexWrap: 'wrap' }}>
            <Card size="small" style={{ flex: 1, minWidth: 140 }}>
              <div style={{ color: '#999', fontSize: 12 }}>涨停家数</div>
              <div style={{ fontSize: 28, fontWeight: 'bold', color: '#cf1322' }}>{data.limit_up_count}</div>
            </Card>
            <Card size="small" style={{ flex: 1, minWidth: 140 }}>
              <div style={{ color: '#999', fontSize: 12 }}>跌停家数</div>
              <div style={{ fontSize: 28, fontWeight: 'bold', color: '#3f8600' }}>{data.limit_down_count}</div>
            </Card>
            <Card size="small" style={{ flex: 1, minWidth: 140 }}>
              <div style={{ color: '#999', fontSize: 12 }}>炸板家数</div>
              <div style={{ fontSize: 28, fontWeight: 'bold', color: '#e8804e' }}>{data.broken_count}</div>
            </Card>
            <Card size="small" style={{ flex: 1, minWidth: 140 }}>
              <div style={{ color: '#999', fontSize: 12 }}>炸板率</div>
              <div style={{ fontSize: 28, fontWeight: 'bold' }}>{data.break_rate.toFixed(1)}%</div>
            </Card>
            <Card size="small" style={{ flex: 1, minWidth: 140 }}>
              <div style={{ color: '#999', fontSize: 12 }}>最高连板</div>
              <div style={{ fontSize: 28, fontWeight: 'bold', color: '#cf1322' }}>{data.max_consecutive}板</div>
            </Card>
            <Card size="small" style={{ flex: 1, minWidth: 140 }}>
              <div style={{ color: '#999', fontSize: 12 }}>一字板 / T字板</div>
              <div style={{ fontSize: 20, fontWeight: 'bold' }}>{data.one_word_count} / {data.t_shape_count}</div>
            </Card>
          </div>

          <Tabs defaultActiveKey="tiers" items={[
            {
              key: 'tiers',
              label: '连板梯队',
              children: (
                <Table
                  dataSource={data.consecutive_tiers}
                  rowKey="days"
                  size="small"
                  pagination={false}
                  columns={[
                    { title: '连板', dataIndex: 'days', render: (v: number) => <Tag color="volcano">{v}板</Tag> },
                    { title: '数量', dataIndex: 'count', render: (v: number) => <strong>{v}</strong> },
                    { title: '股票', dataIndex: 'stocks', render: (stocks: LimitUpStock[]) => (
                      <div style={{ display: 'flex', flexWrap: 'wrap', gap: 4 }}>
                        {stocks.map(s => (
                          <Tag
                            key={s.code}
                            color="orange"
                            style={{ margin: 0, cursor: 'pointer' }}
                            onClick={() => navigate(`/ai/diagnosis?code=${s.code}`)}
                          >
                            {s.name}({s.code})
                          </Tag>
                        ))}
                      </div>
                    )},
                  ]}
                />
              ),
            },
            {
              key: 'ups',
              label: `涨停股 (${data.limit_up_count})`,
              children: (
                <Table
                  dataSource={data.limit_ups}
                  rowKey="code"
                  size="small"
                  scroll={{ y: 500 }}
                  columns={[
                    { title: '代码', dataIndex: 'code', width: 80 },
                    { title: '名称', dataIndex: 'name', width: 100 },
                    { title: '涨跌幅', dataIndex: 'pct_change', width: 80, render: (v: number) => <span style={{ color: '#cf1322' }}>{v.toFixed(2)}%</span> },
                    { title: '连板', dataIndex: 'consecutive_days', width: 60, render: (v: number) => v > 1 ? <Tag color="volcano">{v}板</Tag> : <Tag>首板</Tag> },
                    { title: '换手率', dataIndex: 'turnover', width: 80, render: (v: number) => `${v.toFixed(1)}%` },
                    { title: '成交额', dataIndex: 'amount', width: 100, render: (v: number) => (v / 10000).toFixed(0) + '万' },
                    { title: '振幅', dataIndex: 'amplitude', width: 80, render: (v: number) => `${v.toFixed(1)}%` },
                    { title: '行业', dataIndex: 'industry', width: 100 },
                    { title: '原因', dataIndex: 'reason', width: 120 },
                  ]}
                />
              ),
            },
            {
              key: 'broken',
              label: `炸板股 (${data.broken_count})`,
              children: (
                <Table
                  dataSource={data.broken_boards}
                  rowKey="code"
                  size="small"
                  scroll={{ y: 500 }}
                  columns={[
                    { title: '代码', dataIndex: 'code', width: 80 },
                    { title: '名称', dataIndex: 'name', width: 100 },
                    { title: '涨跌幅', dataIndex: 'pct_change', width: 80, render: (v: number) => <span style={{ color: colorPct(v) }}>{v.toFixed(2)}%</span> },
                    { title: '换手率', dataIndex: 'turnover', width: 80, render: (v: number) => `${v.toFixed(1)}%` },
                    { title: '成交额', dataIndex: 'amount', width: 100, render: (v: number) => (v / 10000).toFixed(0) + '万' },
                    { title: '行业', dataIndex: 'industry', width: 100 },
                  ]}
                />
              ),
            },
            {
              key: 'downs',
              label: `跌停股 (${data.limit_down_count})`,
              children: (
                <Table
                  dataSource={data.limit_downs}
                  rowKey="code"
                  size="small"
                  scroll={{ y: 500 }}
                  columns={[
                    { title: '代码', dataIndex: 'code', width: 80 },
                    { title: '名称', dataIndex: 'name', width: 100 },
                    { title: '涨跌幅', dataIndex: 'pct_change', width: 80, render: (v: number) => <span style={{ color: '#3f8600' }}>{v.toFixed(2)}%</span> },
                    { title: '连续跌停', dataIndex: 'consecutive_days', width: 80, render: (v: number) => <Tag color="green">{v}天</Tag> },
                    { title: '换手率', dataIndex: 'turnover', width: 80, render: (v: number) => `${v.toFixed(1)}%` },
                    { title: '成交额', dataIndex: 'amount', width: 100, render: (v: number) => (v / 10000).toFixed(0) + '万' },
                    { title: '行业', dataIndex: 'industry', width: 100 },
                  ]}
                />
              ),
            },
          ]} />
        </>
      ) : (
        <Empty description="暂无数据，请确认交易时段" />
      )}
    </div>
  )
}
