import { useMemo, useRef, useEffect } from 'react'
import { Card, InputNumber, Typography, Space, Table, Tag, Alert, Select } from 'antd'
import { ArrowUpOutlined, ArrowDownOutlined } from '@ant-design/icons'
import * as echarts from 'echarts'
import { useStockStore } from '../stores/stockStore'
import { usePersistedState } from '../stores/persistedState'

const { Title, Text } = Typography

// 做T 每笔可捕获的价差阶梯（%）
const SPREADS = [0.5, 1, 1.5, 2, 2.5, 3, 3.5, 4, 4.5, 5]

function colorNum(val: number): string {
  return val > 0 ? '#ef4444' : val < 0 ? '#22c55e' : '#9090a8'
}

interface LadderRow {
  spread: number
  perShareReduce: number // 每笔做T 每股降本（元）
  roundProfit: number | null // 每笔做T 总盈利（元，需填持股数）
  newCost: number
  gapToCurrent: number
  roundsToBreakEven: number | null // 成本降至当前价所需笔数
}

export default function TCostOptimizer() {
  const [price, setPrice] = usePersistedState<number>('tcalc.price', 10)
  const [cost, setCost] = usePersistedState<number>('tcalc.cost', 12)
  const [shares, setShares] = usePersistedState<number>('tcalc.shares', 0)
  const [tShares, setTShares] = usePersistedState<number>('tcalc.tShares', 0)

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
    if (item) {
      if (item.cost_price > 0) setCost(item.cost_price)
      if (item.shares > 0) setShares(item.shares)
    }
  }

  const P = Number(price)
  const C = Number(cost)
  const N = Number(shares)
  const M = Number(tShares) > 0 ? Number(tShares) : N
  const hasPos = N > 0
  const hasPrice = isFinite(P) && P > 0
  const hasCost = isFinite(C) && C > 0

  const diff = hasPrice && hasCost ? P - C : 0
  const pctDiff = hasCost ? (diff / C) * 100 : 0
  // 持有不动回到成本所需涨幅
  const holdRisePct = hasPrice && hasCost && P < C ? ((C - P) / P) * 100 : 0
  const inProfit = hasPrice && hasCost && diff > 0
  const flat = hasPrice && hasCost && Math.abs(diff) < 1e-9

  const ladder = useMemo<LadderRow[]>(() => {
    if (!hasPrice || !hasCost) return []
    const out: LadderRow[] = []
    for (const s of SPREADS) {
      const sRatio = s / 100
      const perShareReduce = hasPos && M > 0 ? (M * P * sRatio) / N : NaN
      const newCost = isFinite(perShareReduce) ? C - perShareReduce : NaN
      const roundProfit = M > 0 ? M * P * sRatio : null
      const gapToCurrent = isFinite(newCost) ? newCost - P : NaN
      let rounds: number | null = null
      if (hasPos && M > 0 && isFinite(perShareReduce) && perShareReduce > 0) {
        rounds = C <= P ? 0 : Math.ceil((C - P) / perShareReduce)
      }
      out.push({ spread: s, perShareReduce, roundProfit, newCost, gapToCurrent, roundsToBreakEven: rounds })
    }
    return out
  }, [P, C, N, M, hasPrice, hasCost, hasPos])

  // 图表：各价差下的新成本价
  const chartRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!chartRef.current) return
    const chart = echarts.init(chartRef.current)
    chart.setOption({
      grid: { left: 60, right: 16, top: 30, bottom: 30 },
      tooltip: {
        trigger: 'axis',
        backgroundColor: '#1e2030',
        borderColor: '#33354a',
        textStyle: { color: '#e6e6f0' },
        formatter: (ps: any[]) => {
          const r = ladder[ps[0].dataIndex]
          return `每笔价差 ${r.spread}%<br/>新成本价 ${r.newCost.toFixed(2)}<br/>每股降本 ${r.perShareReduce.toFixed(
            3,
          )}`
        },
      },
      xAxis: {
        type: 'category',
        data: ladder.map((r) => `${r.spread}%`),
        axisLabel: { color: '#9090a8' },
        axisLine: { lineStyle: { color: '#33354a' } },
      },
      yAxis: {
        type: 'value',
        scale: true,
        axisLabel: { color: '#9090a8' },
        splitLine: { lineStyle: { color: '#26283a' } },
      },
      series: [
        {
          type: 'bar',
          data: ladder.map((r) => ({
            value: r.newCost,
            itemStyle: { color: r.newCost <= P ? '#22c55e' : '#ef4444' },
          })),
          barWidth: '60%',
          markLine: {
            silent: true,
            symbol: 'none',
            data: [
              { yAxis: C, name: '成本价', lineStyle: { color: '#f59e0b' }, label: { color: '#f59e0b', formatter: '成本价' } },
              { yAxis: P, name: '当前价', lineStyle: { color: '#60a5fa' }, label: { color: '#60a5fa', formatter: '当前价' } },
            ],
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
  }, [ladder, P, C])

  // 策略提示
  const strategy = useMemo(() => {
    if (!hasPrice || !hasCost) return null
    if (inProfit) {
      return {
        tone: 'success' as const,
        title: '当前浮盈，适合反T锁利',
        text: `当前价高于成本价 ${(diff).toFixed(2)} 元（${pctDiff.toFixed(
          2,
        )}%），可用反T（先卖后买）：在相对高位卖出，回落后再买回，锁定利润并进一步压低成本。`,
      }
    }
    if (flat) {
      return {
        tone: 'info' as const,
        title: '当前持平',
        text: '当前价约等于成本价，做T可开始积累降本空间，正T反T皆可。',
      }
    }
    return {
      tone: 'warning' as const,
      title: '当前浮亏，适合正T/反T降本',
      text: `当前价低于成本价 ${(-diff).toFixed(2)} 元（${pctDiff.toFixed(
        2,
      )}%），持有不动需涨 ${holdRisePct.toFixed(2)}% 才能回本。可用正T（逢低加仓、反弹卖出）或反T（高抛低吸）逐步摊低成本。`,
    }
  }, [hasPrice, hasCost, inProfit, flat, diff, pctDiff, holdRisePct])

  return (
    <div style={{ padding: 16, maxWidth: 1000, margin: '0 auto' }}>
      <Title level={3} style={{ color: '#e6e6f0', marginBottom: 4 }}>
        做T成本优化器
      </Title>
      <Text style={{ color: '#9090a8' }}>
        输入当前股价与持仓成本价，测算持有回本所需涨幅，以及通过做T（高抛低吸/低吸高抛）逐步压低成本的节奏。
      </Text>

      <Card style={{ marginTop: 16, background: '#161826', borderColor: '#26283a' }}>
        <Space size={24} wrap>
          <div>
            <div style={{ color: '#9090a8', marginBottom: 6 }}>从自选股选择</div>
            <Select<string>
              showSearch
              allowClear
              placeholder="选股自动填价/成本/股数"
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
            <InputNumber min={0.01} step={0.01} value={price} onChange={(v) => setPrice(v ?? 0)} style={{ width: 150 }} addonBefore="¥" />
          </div>
          <div>
            <div style={{ color: '#9090a8', marginBottom: 6 }}>持仓成本价（元）</div>
            <InputNumber min={0.01} step={0.01} value={cost} onChange={(v) => setCost(v ?? 0)} style={{ width: 150 }} addonBefore="¥" />
          </div>
          <div>
            <div style={{ color: '#9090a8', marginBottom: 6 }}>持股数量（股，选填）</div>
            <InputNumber min={0} step={100} value={shares} onChange={(v) => setShares(v ?? 0)} style={{ width: 150 }} placeholder="用于算降本" />
          </div>
          <div>
            <div style={{ color: '#9090a8', marginBottom: 6 }}>每笔做T股数（选填）</div>
            <InputNumber min={0} step={100} value={tShares} onChange={(v) => setTShares(v ?? 0)} style={{ width: 150 }} placeholder="默认=持股数" />
          </div>
        </Space>
      </Card>

      {hasPrice && hasCost && (
        <>
          <Card style={{ marginTop: 16, background: '#161826', borderColor: '#26283a' }} styles={{ header: { color: '#e6e6f0' } }}>
            <Space size={32} wrap align="center">
              <div>
                <div style={{ color: '#9090a8', fontSize: 12 }}>持仓状态</div>
                <Tag
                  color={inProfit ? 'red' : flat ? 'default' : 'green'}
                  style={{ fontSize: 14, marginTop: 4 }}
                >
                  {inProfit ? <ArrowUpOutlined /> : flat ? null : <ArrowDownOutlined />}
                  {inProfit ? '浮盈' : flat ? '持平' : '浮亏'} {pctDiff >= 0 ? '+' : ''}
                  {pctDiff.toFixed(2)}%
                </Tag>
              </div>
              <div>
                <div style={{ color: '#9090a8', fontSize: 12 }}>距成本价（元/股）</div>
                <div style={{ color: colorNum(diff), fontSize: 18, fontWeight: 600, marginTop: 4 }}>
                  {diff >= 0 ? '+' : ''}
                  {diff.toFixed(2)}
                </div>
              </div>
              <div>
                <div style={{ color: '#9090a8', fontSize: 12 }}>持有不动回本需涨</div>
                <div style={{ color: inProfit ? '#22c55e' : '#f59e0b', fontSize: 18, fontWeight: 600, marginTop: 4 }}>
                  {inProfit ? '已盈利' : `${holdRisePct.toFixed(2)}%`}
                </div>
              </div>
              <div>
                <div style={{ color: '#9090a8', fontSize: 12 }}>回本目标价（元）</div>
                <div style={{ color: '#e6e6f0', fontSize: 18, fontWeight: 600, marginTop: 4 }}>{C.toFixed(2)}</div>
              </div>
              {hasPos && (
                <div>
                  <div style={{ color: '#9090a8', fontSize: 12 }}>持仓总盈亏（元）</div>
                  <div style={{ color: colorNum(diff * N), fontSize: 18, fontWeight: 600, marginTop: 4 }}>
                    {diff * N >= 0 ? '+' : ''}
                    {(diff * N).toFixed(2)}
                  </div>
                </div>
              )}
            </Space>
          </Card>

          {strategy && (
            <Alert
              style={{ marginTop: 16 }}
              type={strategy.tone}
              showIcon
              message={strategy.title}
              description={strategy.text}
            />
          )}

          <Card
            title="做T降本阶梯（每笔价差对应的新成本价）"
            style={{ marginTop: 16, background: '#161826', borderColor: '#26283a' }}
            styles={{ header: { color: '#e6e6f0' } }}
          >
            <div ref={chartRef} style={{ width: '100%', height: 280 }} />
          </Card>

          <Card style={{ marginTop: 16, background: '#161826', borderColor: '#26283a' }} styles={{ header: { color: '#e6e6f0' } }}>
            <Table<LadderRow>
              size="small"
              pagination={false}
              rowKey="spread"
              dataSource={ladder}
              scroll={{ y: 380 }}
              columns={[
                {
                  title: '每笔做T价差',
                  dataIndex: 'spread',
                  width: 120,
                  render: (s: number) => <Tag color="blue">{s}%</Tag>,
                },
                {
                  title: '每笔降本(元/股)',
                  dataIndex: 'perShareReduce',
                  width: 140,
                  render: (v: number) =>
                    isFinite(v) ? <span style={{ color: '#22c55e' }}>-{v.toFixed(3)}</span> : <span style={{ color: '#9090a8' }}>填持股数</span>,
                },
                {
                  title: '每笔盈利(元)',
                  dataIndex: 'roundProfit',
                  width: 140,
                  render: (v: number | null) =>
                    v != null ? <span style={{ color: '#22c55e' }}>{(v >= 0 ? '+' : '') + v.toFixed(2)}</span> : <span style={{ color: '#9090a8' }}>—</span>,
                },
                {
                  title: '新成本价(元)',
                  dataIndex: 'newCost',
                  width: 140,
                  render: (v: number) =>
                    isFinite(v) ? <span style={{ color: v <= P ? '#22c55e' : '#ef4444' }}>{v.toFixed(2)}</span> : <span style={{ color: '#9090a8' }}>—</span>,
                },
                {
                  title: '新成本距当前价',
                  dataIndex: 'gapToCurrent',
                  width: 150,
                  render: (v: number) =>
                    isFinite(v) ? (
                      <span style={{ color: colorNum(v) }}>
                        {v >= 0 ? '+' : ''}
                        {v.toFixed(2)}
                      </span>
                    ) : (
                      <span style={{ color: '#9090a8' }}>—</span>
                    ),
                },
                {
                  title: '解套需做T笔数',
                  dataIndex: 'roundsToBreakEven',
                  width: 150,
                  render: (v: number | null) => {
                    if (v == null) return <span style={{ color: '#9090a8' }}>填持股数</span>
                    if (v === 0) return <span style={{ color: '#22c55e' }}>已解套</span>
                    return <span style={{ color: '#f59e0b' }}>{v} 笔</span>
                  },
                },
              ]}
            />
            <Text style={{ color: '#9090a8', fontSize: 12, display: 'block', marginTop: 8 }}>
              说明：每笔做T按「价差×做T股数×当前价」产生盈利并等额摊低持仓成本；「解套笔数」指成本降至当前价所需的做T次数（需填写持股数）。
            </Text>
          </Card>
        </>
      )}

      {(!hasPrice || !hasCost) && (
        <Card style={{ marginTop: 16, background: '#161826', borderColor: '#26283a' }}>
          <Text style={{ color: '#9090a8' }}>请输入大于 0 的当前股价与持仓成本价。</Text>
        </Card>
      )}
    </div>
  )
}
