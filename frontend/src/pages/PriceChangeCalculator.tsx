import { useMemo, useRef, useEffect, ReactNode } from 'react'
import { Card, InputNumber, Tag, Typography, Space, Segmented, Select } from 'antd'
import { ArrowUpOutlined, ArrowDownOutlined } from '@ant-design/icons'
import * as echarts from 'echarts'
import { useStockStore } from '../stores/stockStore'
import { usePersistedState } from '../stores/persistedState'

const { Title, Text } = Typography

// 涨停板范围预设（不同板块涨跌停限制不同）
const RANGE_PRESETS: Record<string, { label: string; limit: number }> = {
  main: { label: '主板 ±10%', limit: 10 },
  growth: { label: '创业板/科创 ±20%', limit: 20 },
  bse: { label: '北交所 ±30%', limit: 30 },
}

function colorNum(val: number): string {
  return val > 0 ? '#ef4444' : val < 0 ? '#22c55e' : '#9090a8'
}

interface Row {
  pct: number
  target: number
  delta: number
  totalDelta: number | null
}

export default function PriceChangeCalculator() {
  const [price, setPrice] = usePersistedState<number>('calc.price', 10)
  const [shares, setShares] = usePersistedState<number>('calc.shares', 0)
  const [board, setBoard] = usePersistedState<string>('calc.board', 'main')

  // 自选股联动
  const { watchlist, watchQuotes, fetchWatchlist, fetchWatchQuotes } = useStockStore()
  useEffect(() => {
    fetchWatchlist()
    fetchWatchQuotes()
  }, [fetchWatchlist, fetchWatchQuotes])

  const watchOptions = useMemo(
    () =>
      watchlist.map((item) => {
        const q = watchQuotes.find((x) => x.code === item.stock_code)
        return {
          value: item.stock_code,
          label: `${item.stock_code} ${q?.name ?? ''} ${q ? q.price.toFixed(2) : ''}`,
        }
      }),
    [watchlist, watchQuotes],
  )

  const onPickWatch = (code: string) => {
    const item = watchlist.find((w) => w.stock_code === code)
    const q = watchQuotes.find((x) => x.code === code)
    if (q) setPrice(q.price)
    if (item && item.shares > 0) setShares(item.shares)
  }

  const limit = RANGE_PRESETS[board].limit

  const rows = useMemo<Row[]>(() => {
    const p = Number(price)
    const s = Number(shares)
    if (!isFinite(p) || p <= 0) return []
    const out: Row[] = []
    for (let pct = -limit; pct <= limit; pct += 1) {
      const ratio = pct / 100
      const target = p * (1 + ratio)
      const delta = p * ratio
      out.push({
        pct,
        target,
        delta,
        totalDelta: s > 0 ? delta * s : null,
      })
    }
    return out
  }, [price, shares, limit])

  // 柱状图：每 1% 阶梯的涨跌金额
  const chartRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!chartRef.current) return
    const chart = echarts.init(chartRef.current)
    const maxAbs = Math.max(1e-9, ...rows.map((r) => Math.abs(r.delta)))
    chart.setOption({
      grid: { left: 60, right: 16, top: 20, bottom: 30 },
      tooltip: {
        trigger: 'axis',
        backgroundColor: '#1e2030',
        borderColor: '#33354a',
        textStyle: { color: '#e6e6f0' },
        formatter: (ps: any[]) => {
          const r = rows[ps[0].dataIndex]
          return `${r.pct > 0 ? '+' : ''}${r.pct}% → 目标价 ${r.target.toFixed(
            2,
          )}<br/>涨跌金额/股 ${r.delta >= 0 ? '+' : ''}${r.delta.toFixed(2)}`
        },
      },
      xAxis: {
        type: 'category',
        data: rows.map((r) => `${r.pct > 0 ? '+' : ''}${r.pct}%`),
        axisLabel: { color: '#9090a8', fontSize: 10, interval: 1 },
        axisLine: { lineStyle: { color: '#33354a' } },
      },
      yAxis: {
        type: 'value',
        min: -maxAbs * 1.1,
        max: maxAbs * 1.1,
        axisLabel: { color: '#9090a8' },
        splitLine: { lineStyle: { color: '#26283a' } },
      },
      series: [
        {
          type: 'bar',
          data: rows.map((r) => ({
            value: r.delta,
            itemStyle: { color: r.delta >= 0 ? '#ef4444' : '#22c55e' },
          })),
          barWidth: '70%',
          markLine: {
            silent: true,
            symbol: 'none',
            data: [{ yAxis: 0 }],
            lineStyle: { color: '#5a5d72' },
          },
        },
      ],
    })
    const onResize = () => chart.resize()
    window.addEventListener('resize', onResize)
    return () => {
      window.removeEventListener('resize', onResize)
      chart.dispose()
    }
  }, [rows])

  const baseP = Number(price)
  const hasPrice = isFinite(baseP) && baseP > 0
  const showTotal = Number(shares) > 0

  return (
    <div style={{ padding: 16, maxWidth: 1000, margin: '0 auto' }}>
      <Title level={3} style={{ color: '#e6e6f0', marginBottom: 4 }}>
        股价涨幅计算器
      </Title>
      <Text style={{ color: '#9090a8' }}>
        输入当前股价，按 1% 为阶梯计算 ±涨跌停范围内的目标价与每股涨跌金额；可填持股数估算总盈亏。
      </Text>

      <Card style={{ marginTop: 16, background: '#161826', borderColor: '#26283a' }}>
        <Space size={24} wrap>
          <div>
            <div style={{ color: '#9090a8', marginBottom: 6 }}>从自选股选择</div>
            <Select<string>
              showSearch
              allowClear
              placeholder="选择自选股自动填价"
              value={undefined}
              style={{ width: 220 }}
              options={watchOptions}
              onSelect={onPickWatch}
              filterOption={(input, opt) =>
                (opt?.label as string)?.toLowerCase().includes(input.toLowerCase())
              }
            />
          </div>
          <div>
            <div style={{ color: '#9090a8', marginBottom: 6 }}>当前股价（元）</div>
            <InputNumber
              min={0.01}
              step={0.01}
              value={price}
              onChange={(v) => setPrice(v ?? 0)}
              style={{ width: 160 }}
              addonBefore="¥"
            />
          </div>
          <div>
            <div style={{ color: '#9090a8', marginBottom: 6 }}>持股数量（股，选填）</div>
            <InputNumber
              min={0}
              step={100}
              value={shares}
              onChange={(v) => setShares(v ?? 0)}
              style={{ width: 160 }}
              placeholder="不填则不计算盈亏"
            />
          </div>
          <div>
            <div style={{ color: '#9090a8', marginBottom: 6 }}>涨跌停范围</div>
            <Segmented
              options={Object.entries(RANGE_PRESETS).map(([k, v]) => ({
                label: v.label,
                value: k,
              }))}
              value={board}
              onChange={(v) => setBoard(v as string)}
            />
          </div>
        </Space>
      </Card>

      {hasPrice ? (
        <>
          <Card
            title={`每 1% 阶梯涨跌金额（${RANGE_PRESETS[board].label}）`}
            style={{ marginTop: 16, background: '#161826', borderColor: '#26283a' }}
            styles={{ header: { color: '#e6e6f0' } }}
          >
            <div ref={chartRef} style={{ width: '100%', height: 260 }} />
          </Card>

          <Card
            style={{ marginTop: 16, background: '#161826', borderColor: '#26283a' }}
            styles={{ header: { color: '#e6e6f0' } }}
          >
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                gap: 8,
                marginBottom: 12,
                color: '#9090a8',
                fontSize: 13,
              }}
            >
              <span>当前价 ¥{baseP.toFixed(2)}</span>
              <span style={{ color: '#5a5d72' }}>|</span>
              <span style={{ color: '#9090a8' }}>0% 平盘</span>
            </div>

            <div style={{ display: 'flex', gap: 16 }}>
              <CalcColumn
                title="下跌"
                color="#22c55e"
                icon={<ArrowDownOutlined />}
                rows={rows.filter((r) => r.pct < 0)}
                baseP={baseP}
                showTotal={showTotal}
                gridCols={showTotal ? '64px 1fr 1fr 1fr' : '64px 1fr 1fr'}
              />
              <CalcColumn
                title="上涨"
                color="#ef4444"
                icon={<ArrowUpOutlined />}
                rows={[...rows.filter((r) => r.pct > 0)].reverse()}
                baseP={baseP}
                showTotal={showTotal}
                gridCols={showTotal ? '64px 1fr 1fr 1fr' : '64px 1fr 1fr'}
              />
            </div>
          </Card>
        </>
      ) : (
        <Card style={{ marginTop: 16, background: '#161826', borderColor: '#26283a' }}>
          <Text style={{ color: '#9090a8' }}>请输入大于 0 的当前股价。</Text>
        </Card>
      )}
    </div>
  )
}

function CalcColumn({
  title,
  color,
  icon,
  rows,
  baseP,
  showTotal,
  gridCols,
}: {
  title: string
  color: string
  icon: ReactNode
  rows: Row[]
  baseP: number
  showTotal: boolean
  gridCols: string
}) {
  return (
    <div style={{ flex: 1, minWidth: 0 }}>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 6,
          color,
          fontWeight: 600,
          fontSize: 14,
          marginBottom: 8,
        }}
      >
        {icon}
        <span>{title}</span>
      </div>
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: gridCols,
          gap: 8,
          padding: '4px 8px',
          color: '#9090a8',
          fontSize: 12,
          borderBottom: '1px solid #33354a',        }}
      >
        <span>涨跌幅</span>
        <span>目标价</span>
        <span>每股涨跌</span>
        {showTotal && <span>总盈亏</span>}
      </div>
      {rows.map((r) => (
        <div
          key={r.pct}
          style={{
            display: 'grid',
            gridTemplateColumns: gridCols,
            gap: 8,
            padding: '6px 8px',
            fontSize: 13,
            borderBottom: '1px solid #26283a',
            alignItems: 'center',
          }}
        >
          <Tag color={r.pct > 0 ? 'red' : 'green'} style={{ marginRight: 0 }}>
            {r.pct > 0 ? '+' : ''}
            {r.pct}%
          </Tag>
          <span style={{ color: colorNum(r.target - baseP) }}>{r.target.toFixed(2)}</span>
          <span style={{ color: colorNum(r.delta) }}>
            {r.delta >= 0 ? '+' : ''}
            {r.delta.toFixed(2)}
          </span>
          {showTotal && (
            <span style={{ color: colorNum(r.totalDelta ?? 0) }}>
              {r.totalDelta != null ? (r.totalDelta >= 0 ? '+' : '') + r.totalDelta.toFixed(2) : '-'}
            </span>
          )}
        </div>
      ))}
    </div>
  )
}
