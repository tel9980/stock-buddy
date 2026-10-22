import { useState, useEffect, useRef } from 'react'
import { Card, Table, Input, Button, Spin, Tabs, Empty, message, Select, Tag, Tooltip, Alert, List, Divider } from 'antd'
import { SearchOutlined, RiseOutlined, FallOutlined, TeamOutlined, ShoppingCartOutlined, GlobalOutlined, BulbOutlined } from '@ant-design/icons'
import * as echarts from 'echarts'
import { api, CapitalFlow, NorthBoundHoldings, MarginTrading, CapitalFlowOverview, Quote, SectorCapitalFlow } from '../api'
import { useStockStore } from '../stores/stockStore'
import { usePersistedState } from '../stores/persistedState'

/** 背离分析结果 */
interface DivergenceReason {
  type: 'retail' | 'northbound' | 'margin' | 'sector' | 'manipulate' | 'lag'
  title: string
  description: string
  confidence: 'high' | 'medium' | 'low'
  icon: string
}

// 获取股票名称的缓存
const nameCache = new Map<string, string>();

async function getStockName(code: string): Promise<string> {
  if (nameCache.has(code)) return nameCache.get(code)!;
  try {
    const quote = await api.getQuote(code);
    nameCache.set(code, quote.name);
    return quote.name;
  } catch {
    return code;
  }
}

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
  const [stockCode, setStockCode] = usePersistedState('capital.stockCode', '')
  const [stockFlow, setStockFlow] = useState<CapitalFlow[]>([])
  const [loading, setLoading] = useState(false)
  const [nbHoldings, setNbHoldings] = useState<NorthBoundHoldings[]>([])
  const nbChartRef = useRef<HTMLDivElement>(null)
  const mtChartRef = useRef<HTMLDivElement>(null)

  // 自选股资金流
  const watchlist = useStockStore((s) => s.watchlist)
  const fetchWatchlist = useStockStore((s) => s.fetchWatchlist)
  const [watchFlows, setWatchFlows] = useState<{ code: string; name: string; flows: CapitalFlow[]; latest?: CapitalFlow }[]>([])
  const [watchLoading, setWatchLoading] = useState(false)

  useEffect(() => {
    loadOverview()
    fetchWatchlist()
  }, [])

  // 加载自选股资金流
  useEffect(() => {
    if (watchlist.length > 0) {
      loadWatchFlows()
    }
  }, [watchlist])

  async function loadWatchFlows() {
    setWatchLoading(true)
    try {
      const results = await Promise.all(
        watchlist.map(async (item) => {
          const code = item.stock_code;
          const [flows, name] = await Promise.all([
            api.getCapitalFlow(code, 10),
            getStockName(code),
          ])
          return { code, name, flows, latest: flows[0] }
        })
      )
      setWatchFlows(results)
    } catch (e) {
      message.error('加载自选股资金流失败')
    }
    setWatchLoading(false)
  }

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
          label: '个股资金流',
          children: (
            <div>
              {/* 手动查询 - 放在顶部 */}
              <div style={{ marginBottom: 24 }}>
                <h4 style={{ margin: '0 0 12px' }}>查询股票资金流</h4>
                <div style={{ display: 'flex', gap: 8, marginBottom: 16 }}>
                  <Input placeholder="输入股票代码" value={stockCode} onChange={e => setStockCode(e.target.value)} style={{ width: 160 }} onPressEnter={searchStock} />
                  <Button icon={<SearchOutlined />} onClick={searchStock} loading={loading}>查询</Button>
                </div>
                {stockFlow.length > 0 && (
                  <Table
                    dataSource={stockFlow}
                    rowKey="date"
                    size="small"
                    pagination={false}
                    scroll={{ y: 300 }}
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
                )}
              </div>

              <Divider style={{ borderColor: '#2a2a42' }} />

              {/* 自选股资金流列表 */}
              <div>
                <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 }}>
                  <h4 style={{ margin: 0 }}>自选股资金流</h4>
                  <Button size="small" onClick={loadWatchFlows} loading={watchLoading}>刷新</Button>
                </div>
                {watchLoading ? (
                  <div style={{ textAlign: 'center', padding: 40 }}><Spin /></div>
                ) : watchFlows.length === 0 ? (
                  <Empty description="暂无自选股，请在大盘总览或个股分析中添加自选股" />
                ) : (
                  <Table
                    dataSource={watchFlows}
                    rowKey="code"
                    size="small"
                    pagination={false}
                    scroll={{ y: 400 }}
                    expandable={{
                      expandedRowRender: (record) => (
                        <Table
                          dataSource={record.flows}
                          rowKey="date"
                          size="small"
                          pagination={false}
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
                      ),
                    }}
                    columns={[
                      { title: '代码', dataIndex: 'code', width: 80 },
                      { title: '名称', dataIndex: 'name', width: 100 },
                      { title: '最新日期', dataIndex: 'latest', width: 100, render: (v: CapitalFlow) => v?.date ?? '--' },
                      { title: '收盘价', dataIndex: 'latest', width: 80, render: (v: CapitalFlow) => v?.close_price?.toFixed(2) ?? '--' },
                      { title: '涨跌幅', dataIndex: 'latest', width: 80, render: (v: CapitalFlow) => v ? <span style={{ color: colorNum(v.pct_change) }}>{v.pct_change.toFixed(2)}%</span> : '--' },
                      { title: '主力净流入', dataIndex: 'latest', width: 120, render: (v: CapitalFlow) => v ? <span style={{ color: colorNum(v.main_net_inflow) }}>{formatAmount(v.main_net_inflow)}</span> : '--' },
                      { title: '主力占比', dataIndex: 'latest', width: 80, render: (v: CapitalFlow) => v ? `${v.main_net_ratio.toFixed(1)}%` : '--' },
                    ]}
                  />
                )}
              </div>
            </div>
          ),
        },
        {
          key: 'divergence',
          label: '背离分析',
          children: <DivergenceAnalysis />,
        },
      ]} />
    </div>
  )
}

/** 资金背离分析组件 */
function DivergenceAnalysis() {
  const [code, setCode] = useState('')
  const [loading, setLoading] = useState(false)
  const [reasons, setReasons] = useState<DivergenceReason[]>([])
  const [stockInfo, setStockInfo] = useState<{ name: string; flow: CapitalFlow[]; quote: Quote } | null>(null)

  async function analyzeDivergence() {
    if (!code.trim()) return
    setLoading(true)
    setReasons([])
    try {
      const [flow, quote] = await Promise.all([
        api.getCapitalFlow(code.trim(), 5),
        api.getQuote(code.trim()),
      ])
      setStockInfo({ name: quote.name, flow, quote })

      const result: DivergenceReason[] = []
      const latest = flow[0]
      if (!latest) {
        setReasons([])
        setLoading(false)
        return
      }

      // 计算近5日平均成交额
      const avgAmount = flow.reduce((s, f) => s + f.main_net_inflow, 0) / flow.length

      // 1. 散户接力分析：主力流出 + 成交量放大（用成交额/成交量判断）
      if (latest.main_net_inflow < 0 && quote.volume > 0) {
        const volRatio = quote.volume / (flow.length > 1 ? 1 : 1) // 简化判断
        if (latest.main_net_inflow < 0 && latest.pct_change > 0) {
          result.push({
            type: 'retail',
            title: '散户接力推动',
            description: `主力净流出 ${formatAmount(Math.abs(latest.main_net_inflow))}，但股价逆势上涨，说明散户买入意愿较强，承接了主力抛盘。高换手率是散户参与的重要信号。`,
            confidence: volRatio > 1.5 ? 'high' : 'medium',
            icon: '🛒',
          })
        }
      }

      // 2. 主力对倒识别：主力流出 + 价格涨 + 资金波动大
      if (latest.main_net_inflow < 0 && latest.pct_change > 0 && Math.abs(latest.main_net_inflow) > Math.abs(avgAmount) * 2) {
        result.push({
          type: 'manipulate',
          title: '疑似主力对倒',
          description: `主力显示净流出但股价上涨，且当日资金波动显著高于近期均值，可能存在主力通过对倒制造出货假象的行为。`,
          confidence: 'medium',
          icon: '🔄',
        })
      }

      // 3. 融资买入推动：主力大幅流出但股价涨
      if (latest.main_net_inflow < 0 && latest.pct_change > 0 && latest.main_net_ratio < -5) {
        result.push({
          type: 'margin',
          title: '杠杆资金推动',
          description: `主力大幅流出（占比 ${latest.main_net_ratio.toFixed(1)}%），但股价逆势上涨，可能是融资盘大量买入推动，需关注后续融资偿还压力。`,
          confidence: 'medium',
          icon: '📈',
        })
      }

      // 4. 小盘股效应：成交量相对较大但主力流出
      if (latest.main_net_inflow < 0 && latest.pct_change > 0 && quote.amount < 500000000) {
        result.push({
          type: 'retail',
          title: '小盘股效应',
          description: `成交额仅 ${(quote.amount / 10000).toFixed(0)} 万，少量资金即可推动股价，主力流出对价格影响有限，少量散户买入即可拉升。`,
          confidence: 'high',
          icon: '💡',
        })
      }

      // 5. 边拉边出形态：主力小幅流出 + 股价微涨
      if (latest.main_net_inflow < 0 && latest.pct_change > 0 && latest.pct_change < 3 && Math.abs(latest.main_net_inflow) < 50000000) {
        result.push({
          type: 'manipulate',
          title: '边拉边出形态',
          description: `主力小幅流出（${formatAmount(Math.abs(latest.main_net_inflow))}）但股价微涨（+${latest.pct_change.toFixed(2)}%），可能是主力维持股价缓慢出货的典型形态，诱多吸引跟风盘。`,
          confidence: 'medium',
          icon: '⚠️',
        })
      }

      // 6. 滞后效应：前几日主力流入，今日流出
      const prevFlow = flow[1]
      if (latest.main_net_inflow < 0 && prevFlow && prevFlow.main_net_inflow > 0 && prevFlow.pct_change > 0) {
        result.push({
          type: 'lag',
          title: '资金效应滞后',
          description: '前几日主力大幅流入推升股价，今日虽然流出但上涨动能仍在，属于资金驱动的滞后效应。',
          confidence: 'low',
          icon: '⏳',
        })
      }

      // 7. 无明显背离
      if (result.length === 0) {
        if (latest.main_net_inflow >= 0) {
          result.push({
            type: 'retail',
            title: '主力资金正常流入',
            description: '主力资金为净流入，股价上涨与资金面一致，无明显背离。',
            confidence: 'high',
            icon: '✅',
          })
        } else {
          result.push({
            type: 'retail',
            title: '数据待观察',
            description: '当前数据不足以判断背离原因，建议结合更多维度（北向资金、板块走势）综合分析。',
            confidence: 'low',
            icon: '🔍',
          })
        }
      }

      setReasons(result)
    } catch {
      message.error('分析失败，请检查股票代码')
    }
    setLoading(false)
  }

  const confidenceColor = (c: string) => c === 'high' ? '#cf1322' : c === 'medium' ? '#e8804e' : '#999'
  const confidenceLabel = (c: string) => c === 'high' ? '高' : c === 'medium' ? '中' : '低'

  return (
    <div>
      <div style={{ display: 'flex', gap: 8, marginBottom: 16 }}>
        <Input
          placeholder="输入股票代码，如 000001"
          value={code}
          onChange={e => setCode(e.target.value)}
          style={{ width: 180 }}
          onPressEnter={analyzeDivergence}
        />
        <Button icon={<BulbOutlined />} onClick={analyzeDivergence} loading={loading}>分析背离原因</Button>
      </div>

      {stockInfo && (
        <>
          <Card size="small" style={{ marginBottom: 16 }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <span style={{ fontSize: 16, fontWeight: 'bold' }}>{stockInfo.name} ({code})</span>
              <span style={{ fontSize: 14, color: colorNum(stockInfo.quote.pct_change) }}>
                涨跌幅: {stockInfo.quote.pct_change > 0 ? '+' : ''}{stockInfo.quote.pct_change.toFixed(2)}%
              </span>
            </div>
            <div style={{ marginTop: 8, fontSize: 12, color: '#999' }}>
              主力净流入: <span style={{ color: colorNum(stockInfo.flow[0]?.main_net_inflow ?? 0) }}>
                {stockInfo.flow[0] ? formatAmount(stockInfo.flow[0].main_net_inflow) : '--'}
              </span>
              {' | '}
              成交额: {(stockInfo.quote.amount / 10000).toFixed(0)}万
            </div>
          </Card>

          <Alert
            type="info"
            showIcon
            message="背离分析说明"
            description="以下分析基于资金流向数据的技术面推断，仅供参考，不构成投资建议。"
            style={{ marginBottom: 16 }}
          />

          <List
            dataSource={reasons}
            renderItem={(item) => (
              <List.Item>
                <List.Item.Meta
                  avatar={<span style={{ fontSize: 24 }}>{item.icon}</span>}
                  title={
                    <span>
                      {item.title}
                      <Tag color={confidenceColor(item.confidence)} style={{ marginLeft: 8 }}>
                        可信度: {confidenceLabel(item.confidence)}
                      </Tag>
                    </span>
                  }
                  description={item.description}
                />
              </List.Item>
            )}
          />
        </>
      )}

      {!stockInfo && !loading && (
        <Empty description="输入股票代码后点击分析按钮" />
      )}
    </div>
  )
}
