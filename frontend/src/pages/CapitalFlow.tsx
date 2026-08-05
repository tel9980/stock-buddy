import { useState, useEffect, useRef } from 'react'
import { Card, Table, Input, Button, Spin, Tabs, Empty, message } from 'antd'
import { SearchOutlined } from '@ant-design/icons'
import * as echarts from 'echarts'
import { api, CapitalFlow, NorthBoundHoldings, MarginTrading, CapitalFlowOverview } from '../api'

function formatAmount(val: number): string {
  const w = val / 10000
  if (Math.abs(w) >= 10000) return (w / 10000).toFixed(2) + '亿'
  return w.toFixed(0) + '万'
}

/** 数据单位为元，直接格式化为亿 */
function formatYi(val: number): string {
  return (val / 1e8).toFixed(0) + '亿'
}

function colorNum(val: number): string {
  return val > 0 ? '#cf1322' : val < 0 ? '#3f8600' : '#666'
}

export default function CapitalFlowPage() {
  const [overview, setOverview] = useState<CapitalFlowOverview | null>(null)
  const [stockCode, setStockCode] = useState('')
  const [stockFlow, setStockFlow] = useState<CapitalFlow[]>([])
  const [loading, setLoading] = useState(false)
  const [nbHoldings, setNbHoldings] = useState<NorthBoundHoldings[]>([])
  const nbChartRef = useRef<HTMLDivElement>(null)
  const mtChartRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    loadOverview()
  }, [])

  async function loadOverview() {
    setLoading(true)
    try {
      const [ov, nb, mt] = await Promise.all([
        api.getCapitalOverview(),
        api.getNorthBoundHoldings(),
        api.getMarginTrading(20),
      ])
      setOverview(ov)
      setNbHoldings(nb)
      setTimeout(() => {
        renderHoldingsChart(nb.reverse())
        renderMarginChart(mt.reverse())
      }, 100)
    } catch (e) {
      message.error('加载资金流向数据失败')
    }
    setLoading(false)
  }

  function renderHoldingsChart(data: NorthBoundHoldings[]) {
    if (!nbChartRef.current) return
    const chart = echarts.init(nbChartRef.current)
    chart.setOption({
      tooltip: { trigger: 'axis' },
      legend: { top: 0, textStyle: { color: '#999', fontSize: 11 } },
      grid: { left: 60, right: 20, top: 30, bottom: 30 },
      xAxis: { type: 'category', data: data.map(d => d.date.slice(0, 7)), axisLabel: { color: '#999', fontSize: 10 } },
      yAxis: { type: 'value', axisLabel: { color: '#999', formatter: (v: number) => v.toFixed(0) + '亿' } },
      series: [
        { name: '持股市值', type: 'bar', data: data.map(d => d.hold_market_cap / 1e8), itemStyle: { color: '#1890ff' } },
        { name: '增减持市值', type: 'bar', data: data.map(d => d.add_market_cap / 1e8), itemStyle: { color: '#e8804e' } },
      ],
    })
    return () => chart.dispose()
  }

  function renderMarginChart(data: MarginTrading[]) {
    if (!mtChartRef.current) return
    const chart = echarts.init(mtChartRef.current)
    chart.setOption({
      tooltip: { trigger: 'axis' },
      legend: { top: 0, textStyle: { color: '#999', fontSize: 11 } },
      grid: { left: 60, right: 60, top: 30, bottom: 30 },
      xAxis: { type: 'category', data: data.map(d => d.date.slice(5)), axisLabel: { color: '#999', fontSize: 10 } },
      yAxis: [
        { type: 'value', axisLabel: { color: '#999', formatter: (v: number) => (v / 1e8).toFixed(0) + '亿' } },
        { type: 'value', axisLabel: { color: '#999', formatter: (v: number) => (v / 1e8).toFixed(0) + '亿' } },
      ],
      series: [
        { name: '融资余额', type: 'line', data: data.map(d => d.margin_balance), lineStyle: { color: '#cf1322' }, symbol: 'none', yAxisIndex: 0 },
        { name: '融券余额', type: 'line', data: data.map(d => d.short_balance), lineStyle: { color: '#3f8600' }, symbol: 'none', yAxisIndex: 0 },
        { name: '融资净买入', type: 'bar', data: data.map(d => d.margin_net_buy), itemStyle: { color: '#e8804e' }, yAxisIndex: 1 },
      ],
    })
    return () => chart.dispose()
  }

  async function searchStock() {
    if (!stockCode) return
    setLoading(true)
    try {
      const data = await api.getCapitalFlow(stockCode, 30)
      setStockFlow(data)
    } catch {
      message.error('查询失败')
    }
    setLoading(false)
  }

  return (
    <div>
      <h2 style={{ margin: '0 0 16px' }}>资金流向</h2>

      {overview && (
        <div style={{ display: 'flex', gap: 12, marginBottom: 16, flexWrap: 'wrap' }}>
          <Card size="small" style={{ flex: 1, minWidth: 200 }}>
            <div style={{ color: '#999', fontSize: 12 }}>北向持股（季度）</div>
            {overview.north_bound_holdings ? (
              <>
                <div style={{ fontSize: 24, fontWeight: 'bold' }}>
                  {formatYi(overview.north_bound_holdings.hold_market_cap)}
                </div>
                <div style={{ fontSize: 12, color: colorNum(overview.north_bound_holdings.change_rate) }}>
                  环比 {(overview.north_bound_holdings.change_rate ?? 0) > 0 ? '+' : ''}{overview.north_bound_holdings.change_rate?.toFixed(2)}%
                </div>
              </>
            ) : (
              <div style={{ fontSize: 24, fontWeight: 'bold', color: '#999' }}>无数据</div>
            )}
          </Card>
          <Card size="small" style={{ flex: 1, minWidth: 200 }}>
            <div style={{ color: '#999', fontSize: 12 }}>主力资金净流入（今日）</div>
            {overview.market_capital_flow ? (
              <div style={{ fontSize: 24, fontWeight: 'bold', color: colorNum(overview.market_capital_flow.main_net_inflow) }}>
                {formatYi(overview.market_capital_flow.main_net_inflow)}
              </div>
            ) : (
              <div style={{ fontSize: 24, fontWeight: 'bold', color: '#999' }}>无数据</div>
            )}
          </Card>
          <Card size="small" style={{ flex: 1, minWidth: 200 }}>
            <div style={{ color: '#999', fontSize: 12 }}>融资融券余额</div>
            <div style={{ fontSize: 20, fontWeight: 'bold' }}>
              {overview.margin_trading ? formatYi(overview.margin_trading.total_balance) : '--'}
            </div>
          </Card>
          <Card size="small" style={{ flex: 1, minWidth: 200 }}>
            <div style={{ color: '#999', fontSize: 12 }}>融资净买入</div>
            <div style={{ fontSize: 20, fontWeight: 'bold', color: (overview.margin_trading?.margin_net_buy ?? 0) > 0 ? '#cf1322' : '#3f8600' }}>
              {formatYi(overview.margin_trading?.margin_net_buy ?? 0)}
            </div>
          </Card>
        </div>
      )}

      <div style={{ display: 'flex', gap: 16, marginBottom: 16 }}>
        <Card title="北向持股趋势（季度）" size="small" style={{ flex: 1 }}>
          {nbHoldings.length > 0 ? (
            <div ref={nbChartRef} style={{ height: 250 }} />
          ) : (
            <Empty description="暂无北向持股数据" style={{ padding: '40px 0' }} />
          )}
        </Card>
        <Card title="融资融券趋势" size="small" style={{ flex: 1 }}>
          <div ref={mtChartRef} style={{ height: 250 }} />
        </Card>
      </div>

      <Tabs defaultActiveKey="sectors" items={[
        {
          key: 'sectors',
          label: '行业板块资金流',
          children: (
            <Table
              dataSource={overview?.sector_flows ?? []}
              rowKey="code"
              size="small"
              pagination={false}
              scroll={{ y: 400 }}
              columns={[
                { title: '板块', dataIndex: 'name', width: 120 },
                { title: '涨跌幅', dataIndex: 'pct_change', width: 80, render: (v: number) => <span style={{ color: colorNum(v) }}>{v.toFixed(2)}%</span> },
                { title: '主力净流入', dataIndex: 'main_net_inflow', width: 120, render: (v: number) => <span style={{ color: colorNum(v) }}>{formatAmount(v)}</span> },
                { title: '主力占比', dataIndex: 'main_net_ratio', width: 80, render: (v: number) => `${v.toFixed(1)}%` },
                { title: '超大单', dataIndex: 'huge_net_inflow', width: 100, render: (v: number) => <span style={{ color: colorNum(v) }}>{formatAmount(v)}</span> },
                { title: '大单', dataIndex: 'large_net_inflow', width: 100, render: (v: number) => <span style={{ color: colorNum(v) }}>{formatAmount(v)}</span> },
                { title: '中单', dataIndex: 'medium_net_inflow', width: 100, render: (v: number) => <span style={{ color: colorNum(v) }}>{formatAmount(v)}</span> },
                { title: '小单', dataIndex: 'small_net_inflow', width: 100, render: (v: number) => <span style={{ color: colorNum(v) }}>{formatAmount(v)}</span> },
              ]}
            />
          ),
        },
        {
          key: 'stock',
          label: '个股资金流查询',
          children: (
            <div>
              <div style={{ display: 'flex', gap: 8, marginBottom: 16 }}>
                <Input placeholder="输入股票代码" value={stockCode} onChange={e => setStockCode(e.target.value)} style={{ width: 160 }} />
                <Button icon={<SearchOutlined />} onClick={searchStock} loading={loading}>查询</Button>
              </div>
              <Table
                dataSource={stockFlow}
                rowKey="date"
                size="small"
                pagination={false}
                scroll={{ y: 400 }}
                columns={[
                  { title: '日期', dataIndex: 'date', width: 100 },
                  { title: '收盘价', dataIndex: 'close_price', width: 80 },
                  { title: '涨跌幅', dataIndex: 'pct_change', width: 80, render: (v: number) => <span style={{ color: colorNum(v) }}>{v.toFixed(2)}%</span> },
                  { title: '主力净流入', dataIndex: 'main_net_inflow', width: 120, render: (v: number) => <span style={{ color: colorNum(v) }}>{formatAmount(v)}</span> },
                  { title: '主力占比', dataIndex: 'main_net_ratio', width: 80, render: (v: number) => `${v.toFixed(1)}%` },
                  { title: '超大单', dataIndex: 'huge_net_inflow', width: 100, render: (v: number) => <span style={{ color: colorNum(v) }}>{formatAmount(v)}</span> },
                  { title: '大单', dataIndex: 'large_net_inflow', width: 100, render: (v: number) => <span style={{ color: colorNum(v) }}>{formatAmount(v)}</span> },
                  { title: '中单', dataIndex: 'medium_net_inflow', width: 100, render: (v: number) => <span style={{ color: colorNum(v) }}>{formatAmount(v)}</span> },
                  { title: '小单', dataIndex: 'small_net_inflow', width: 100, render: (v: number) => <span style={{ color: colorNum(v) }}>{formatAmount(v)}</span> },
                ]}
              />
            </div>
          ),
        },
      ]} />
    </div>
  )
}
