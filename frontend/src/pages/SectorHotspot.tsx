import { useState, useEffect, useRef } from 'react'
import { Card, Tag, Tabs, Table, Spin, Empty, Alert, Button, Select, Switch, Tooltip } from 'antd'
import { ReloadOutlined, FireOutlined, RiseOutlined, FallOutlined } from '@ant-design/icons'
import * as echarts from 'echarts'
import { api, SectorHotspotAnalysis, SectorHotspotItem, SectorLeaderFundamental } from '../api'
import MarkdownView from '../components/MarkdownView'
import { usePersistedState } from '../stores/persistedState'

function colorNum(val: number): string {
  return val > 0 ? '#ef4444' : val < 0 ? '#22c55e' : '#9090a8'
}
function formatYi(val: number): string {
  return (val / 1e8).toFixed(2) + ' 亿'
}

const LOGIC_COLOR: Record<string, string> = {
  业绩驱动: 'green',
  '业绩+资金驱动': 'green',
  业绩支撑: 'cyan',
  资金驱动: 'orange',
  '题材炒作': 'magenta',
  '题材/游资炒作': 'magenta',
  普涨跟随: 'default',
}

function logicColor(logic: string): string {
  return LOGIC_COLOR[logic] ?? 'default'
}

function fundamentalTable(lf: SectorLeaderFundamental) {
  const rows = [
    { k: '最新报告期', v: lf.report_date || '-' },
    { k: '营收同比', v: lf.revenue_yoy != null ? `${lf.revenue_yoy.toFixed(1)}%` : '-', c: lf.revenue_yoy },
    { k: '归母净利润同比', v: lf.net_profit_yoy != null ? `${lf.net_profit_yoy.toFixed(1)}%` : '-', c: lf.net_profit_yoy },
    { k: 'ROE', v: lf.roe != null ? `${lf.roe.toFixed(1)}%` : '-', c: lf.roe },
    { k: '毛利率', v: lf.gross_margin != null ? `${lf.gross_margin.toFixed(1)}%` : '-', c: lf.gross_margin },
    { k: '净利率', v: lf.net_margin != null ? `${lf.net_margin.toFixed(1)}%` : '-', c: lf.net_margin },
  ]
  return (
    <Table
      size="small"
      pagination={false}
      rowKey="k"
      dataSource={rows}
      columns={[
        { title: '指标', dataIndex: 'k', width: 130 },
        {
          title: '数值',
          dataIndex: 'v',
          render: (v: string, r: { c?: number | null }) =>
            r.c != null ? <span style={{ color: colorNum(r.c) }}>{v}</span> : <span>{v}</span>,
        },
      ]}
    />
  )
}

function HotspotCard({ item }: { item: SectorHotspotItem }) {
  const inflowYi = item.main_net_inflow / 1e8
  const tabs = [
    {
      key: 'tech',
      label: '技术面',
      children: (
        <div>
          <MarkdownView content={item.reason.technical} />
          <div style={{ marginTop: 12, display: 'flex', flexWrap: 'wrap', gap: 8 }}>
            {item.reason.factors
              .filter((f) => f.category === '技术面' || f.category === '资金面')
              .map((f, i) => (
                <Tooltip key={i} title={f.detail}>
                  <Tag color="blue">
                    {f.name} {f.detail}
                  </Tag>
                </Tooltip>
              ))}
          </div>
        </div>
      ),
    },
    {
      key: 'fund',
      label: '基本面',
      children: (
        <div>
          <MarkdownView content={item.reason.fundamental} />
          {item.leader_fundamental ? (
            <div style={{ marginTop: 12 }}>
              <div style={{ color: '#9090a8', fontSize: 12, marginBottom: 6 }}>
                领涨股 {item.leader_fundamental.code} {item.leader_fundamental.name}
                {item.leader_fundamental.pct_change != null && (
                  <span style={{ color: colorNum(item.leader_fundamental.pct_change) }}>
                    （{item.leader_fundamental.pct_change.toFixed(2)}%）
                  </span>
                )}
              </div>
              {fundamentalTable(item.leader_fundamental)}
            </div>
          ) : (
            <Alert
              style={{ marginTop: 12 }}
              type="info"
              showIcon
              message="暂无领涨股基本面数据"
              description="财务接口受限或领涨股未知，基本面结论仅供参考。"
            />
          )}
        </div>
      ),
    },
    {
      key: 'catalyst',
      label: (
        <span>
          催化剂
          {item.dragon_tiger_hit && <Tag color="red" style={{ marginLeft: 6 }}>龙虎榜</Tag>}
        </span>
      ),
      children: <MarkdownView content={item.reason.catalyst} />,
    },
  ]

  return (
    <Card
      size="small"
      style={{ marginBottom: 16 }}
      title={
        <span>
          <span style={{ color: 'var(--primary)', marginRight: 8 }}>#{item.rank}</span>
          {item.name}
          <Tag color={item.sector_type === '行业' ? 'geekblue' : 'purple'} style={{ marginLeft: 8 }}>
            {item.sector_type}
          </Tag>
          <Tag color={logicColor(item.reason.logic)} style={{ marginLeft: 4 }}>
            {item.reason.logic}
          </Tag>
        </span>
      }
    >
      <div style={{ display: 'flex', gap: 24, marginBottom: 12, flexWrap: 'wrap' }}>
        <div>
          <div style={{ color: '#9090a8', fontSize: 12 }}>板块涨幅</div>
          <div style={{ fontSize: '1.2rem', fontWeight: 700, color: colorNum(item.change_pct) }}>
            {item.change_pct.toFixed(2)}%
          </div>
        </div>
        <div>
          <div style={{ color: '#9090a8', fontSize: 12 }}>热度评分</div>
          <div style={{ fontSize: '1.2rem', fontWeight: 700 }}>{item.strength_score.toFixed(0)}</div>
        </div>
        <div>
          <div style={{ color: '#9090a8', fontSize: 12 }}>主力净流入</div>
          <div style={{ fontSize: '1.2rem', fontWeight: 700, color: colorNum(inflowYi) }}>
            {formatYi(item.main_net_inflow)}
          </div>
        </div>
        <div>
          <div style={{ color: '#9090a8', fontSize: 12 }}>上涨广度</div>
          <div style={{ fontSize: '1.2rem', fontWeight: 700 }}>
            {item.up_count}
            <span style={{ color: '#22c55e' }}> / {item.down_count}</span>
          </div>
        </div>
      </div>
      <Tabs defaultActiveKey="tech" items={tabs} size="small" />
    </Card>
  )
}

export default function SectorHotspotPage() {
  const [data, setData] = useState<SectorHotspotAnalysis | null>(null)
  const [loading, setLoading] = useState(false)
  const [top, setTop] = usePersistedState('sector.top', 12)
  const [withFund, setWithFund] = usePersistedState('sector.withFund', true)
  const chartRef = useRef<HTMLDivElement>(null)
  const chartInst = useRef<echarts.ECharts | null>(null)

  async function load() {
    setLoading(true)
    try {
      const res = await api.getSectorHotspot(top, withFund)
      setData(res)
    } catch (e) {
      setData(null)
    }
    setLoading(false)
  }

  useEffect(() => {
    load()
  }, [top, withFund])

  // 柱状图：涨幅 + 主力净流入(亿)
  useEffect(() => {
    if (!chartRef.current) return
    if (!chartInst.current) chartInst.current = echarts.init(chartRef.current)
    const chart = chartInst.current
    if (!data) {
      chart.clear()
      return
    }
    const topN = [...data.hot_sectors]
      .sort((a, b) => b.change_pct - a.change_pct)
      .slice(0, 10)
      .reverse()
    const names = topN.map((s) => s.name)
    const changes = topN.map((s) => s.change_pct)
    const inflows = topN.map((s) => +(s.main_net_inflow / 1e8).toFixed(2))
    chart.setOption(
      {
        backgroundColor: 'transparent',
        tooltip: { trigger: 'axis', axisPointer: { type: 'shadow' } },
        legend: { data: ['涨幅%', '主力净流入(亿)'], textStyle: { color: '#9090a8' } },
        grid: { left: 100, right: 70, top: 40, bottom: 30 },
        xAxis: {
          type: 'value',
          axisLabel: { color: '#9090a8' },
          splitLine: { lineStyle: { color: '#2a2a42' } },
        },
        yAxis: [
          {
            type: 'category',
            data: names,
            axisLabel: { color: '#e0e0e8' },
            axisLine: { lineStyle: { color: '#2a2a42' } },
          },
          {
            type: 'value',
            name: '亿',
            nameTextStyle: { color: '#9090a8' },
            axisLabel: { color: '#9090a8' },
            splitLine: { show: false },
          },
        ],
        series: [
          {
            name: '涨幅%',
            type: 'bar',
            data: changes.map((v) => ({
              value: v,
              itemStyle: { color: v >= 0 ? '#ef4444' : '#22c55e' },
            })),
            label: { show: true, position: 'right', color: '#9090a8', formatter: '{c}%' },
          },
          {
            name: '主力净流入(亿)',
            type: 'bar',
            yAxisIndex: 1,
            data: inflows.map((v) => ({
              value: v,
              itemStyle: { color: v >= 0 ? '#e8804e' : '#3f8600' },
            })),
          },
        ],
      },
      true,
    )
  }, [data])

  useEffect(() => {
    return () => {
      chartInst.current?.dispose()
      chartInst.current = null
    }
  }, [])

  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 16, marginBottom: 16, flexWrap: 'wrap' }}>
        <h2 style={{ margin: 0, display: 'flex', alignItems: 'center', gap: 8 }}>
          <FireOutlined style={{ color: 'var(--primary)' }} /> 市场热点板块
        </h2>
        <Select value={top} style={{ width: 110 }} onChange={setTop}
          options={[{ value: 12, label: 'Top 12' }, { value: 16, label: 'Top 16' }, { value: 20, label: 'Top 20' }]} />
        <span style={{ color: '#9090a8', fontSize: 13, display: 'flex', alignItems: 'center', gap: 6 }}>
          基本面分析
          <Switch checked={withFund} onChange={setWithFund} />
        </span>
        <Button icon={<ReloadOutlined />} onClick={load} loading={loading}>
          刷新
        </Button>
      </div>

      {loading && !data && (
        <div className="chart-loading">
          <Spin tip="正在聚合板块排行、资金流与领涨股基本面..." />
        </div>
      )}

      {data && (
        <>
          <Alert
            type="info"
            showIcon
            style={{ marginBottom: 16 }}
            message={
              <span>
                共识别 <b>{data.industry_count}</b> 个行业、<b>{data.concept_count}</b> 个概念热点板块
              </span>
            }
            description={<MarkdownView content={data.conclusion} />}
          />

          <Card size="small" title="热点板块涨幅 vs 主力净流入" style={{ marginBottom: 16 }}>
            <div ref={chartRef} style={{ width: '100%', height: 420 }} />
          </Card>

          {data.hot_sectors.length === 0 ? (
            <Empty description="暂无热点板块数据" />
          ) : (
            <div
              style={{
                display: 'grid',
                gridTemplateColumns: 'repeat(auto-fill, minmax(440px, 1fr))',
                gap: 16,
              }}
            >
              {data.hot_sectors.map((item) => (
                <HotspotCard key={item.code} item={item} />
              ))}
            </div>
          )}
        </>
      )}
    </div>
  )
}
