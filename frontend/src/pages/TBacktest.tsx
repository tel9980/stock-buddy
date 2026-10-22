import { useState, useMemo, useRef, useEffect, useCallback } from 'react'
import { Card, Input, InputNumber, Select, Switch, Slider, Button, Tabs, Table, Alert, Tag, Space, Typography, Spin } from 'antd'
import { PlayCircleOutlined, ExperimentOutlined } from '@ant-design/icons'
import * as echarts from 'echarts'
import { api } from '../api'
import type {
  TBacktestResult,
  TScanItem,
  TStrategyConfig,
  TTrade,
  TradeLeg,
  ExitMode,
  TMode,
} from '../api'
import { useStockStore } from '../stores/stockStore'
import { useTBacktestStore } from '../stores/tBacktestStore'

const { Text, Title } = Typography

const CARD_STYLE = { background: '#161826', borderColor: '#26283a' } as const
const LABEL: React.CSSProperties = { color: '#9090a8', marginBottom: 6, fontSize: 12 }
const UP = '#f5455c'
const DOWN = '#0ecb81'

/** 收益率配色：A 股习惯红涨绿跌 */
function colorOf(v: number): string {
  return v > 0 ? UP : v < 0 ? DOWN : '#9090a8'
}

function signed(v: number, digits = 2): string {
  return `${v >= 0 ? '+' : ''}${v.toFixed(digits)}`
}

/** 核心指标卡 */
function Metric({ label, value, sub, color }: { label: string; value: string; sub?: string; color?: string }) {
  return (
    <div
      style={{
        flex: '1 1 130px',
        minWidth: 130,
        padding: '12px 14px',
        background: '#1b1e2e',
        border: '1px solid #26283a',
        borderRadius: 8,
      }}
    >
      <div style={{ color: '#9090a8', fontSize: 12, marginBottom: 6 }}>{label}</div>
      <div style={{ color: color ?? '#e6e6f0', fontSize: 22, fontWeight: 600, lineHeight: 1.2 }}>{value}</div>
      {sub && <div style={{ color: '#6b6d80', fontSize: 11, marginTop: 4 }}>{sub}</div>}
    </div>
  )
}

export default function TBacktest() {
  const { watchlist, watchQuotes } = useStockStore()

  // 表单与结果全部托管在全局 store，路由切换/刷新都不丢失
  const code = useTBacktestStore((s) => s.code)
  const period = useTBacktestStore((s) => s.period)
  const limit = useTBacktestStore((s) => s.limit)
  const cfg = useTBacktestStore((s) => s.cfg)
  const showAdvanced = useTBacktestStore((s) => s.showAdvanced)
  const activeTab = useTBacktestStore((s) => s.activeTab)
  const result = useTBacktestStore((s) => s.result)
  const scan = useTBacktestStore((s) => s.scan)
  const setCode = useTBacktestStore((s) => s.setCode)
  const setPeriod = useTBacktestStore((s) => s.setPeriod)
  const setLimit = useTBacktestStore((s) => s.setLimit)
  const patch = useTBacktestStore((s) => s.patchConfig)
  const setShowAdvanced = useTBacktestStore((s) => s.setShowAdvanced)
  const setActiveTab = useTBacktestStore((s) => s.setActiveTab)
  const setResult = useTBacktestStore((s) => s.setResult)
  const setScan = useTBacktestStore((s) => s.setScan)
  const resetConfig = useTBacktestStore((s) => s.resetConfig)

  const [loading, setLoading] = useState(false)
  const [scanLoading, setScanLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // 自选股下拉
  const watchOptions = useMemo(() => {
    return watchlist.map((w) => {
      const q = watchQuotes.find((x) => x.code === w.stock_code)
      return {
        value: w.stock_code,
        label: q ? `${w.stock_code} ${q.name}` : w.stock_code,
      }
    })
  }, [watchlist, watchQuotes])

  const runBacktest = useCallback(async () => {
    if (!code.trim()) return
    setLoading(true)
    setError(null)
    try {
      const r = await api.runTBacktest(code.trim(), { period, limit, config: cfg })
      setResult(r)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }, [code, period, limit, cfg, setResult])

  const runScan = useCallback(async () => {
    if (!code.trim()) return
    setScanLoading(true)
    setError(null)
    try {
      const r = await api.scanTParams(code.trim(), period, limit)
      setScan(r)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setScanLoading(false)
    }
  }, [code, period, limit, setScan])

  return (
    <div style={{ padding: 16, maxWidth: 1280, margin: '0 auto' }}>
      <Title level={4} style={{ color: '#e6e6f0', marginBottom: 4 }}>
        做T策略回测 · KDJ + 布林带六步法则
      </Title>
      <Text style={{ color: '#6b6d80', fontSize: 12 }}>
        默认参数 = 中轨止盈 + 缩量确认 + 双模。回测与实盘扫描共用同一套信号逻辑。
      </Text>

      {/* ── 参数区 ── */}
      <Card style={{ ...CARD_STYLE, marginTop: 16 }} styles={{ header: { color: '#e6e6f0' } }}>
        <Space size={20} wrap align="start">
          <div>
            <div style={LABEL}>股票代码</div>
            <Input
              value={code}
              onChange={(e) => setCode(e.target.value)}
              style={{ width: 130 }}
              placeholder="如 000001"
            />
          </div>
          {watchOptions.length > 0 && (
            <div>
              <div style={LABEL}>从自选股选择</div>
              <Select
                showSearch
                placeholder="选择自选股"
                style={{ width: 180 }}
                options={watchOptions}
                onChange={(v: string) => setCode(v)}
                filterOption={(input, opt) => (opt?.label as string)?.toLowerCase().includes(input.toLowerCase())}
              />
            </div>
          )}
          <div>
            <div style={LABEL}>周期</div>
            <Select
              value={period}
              onChange={setPeriod}
              style={{ width: 110 }}
              options={[
                { value: 'daily', label: '日线（推荐）' },
                { value: '60', label: '60 分钟' },
                { value: '30', label: '30 分钟' },
                { value: '15', label: '15 分钟' },
              ]}
            />
          </div>
          <div>
            <div style={LABEL}>K 线根数</div>
            <InputNumber min={100} max={5000} step={100} value={limit} onChange={(v) => setLimit(v ?? 500)} style={{ width: 110 }} />
          </div>
          <div>
            <div style={LABEL}>做T方向</div>
            <Select
              value={cfg.t_mode}
              onChange={(v: TMode) => patch({ t_mode: v })}
              style={{ width: 150 }}
              options={[
                { value: 'both', label: '双模（自动）' },
                { value: 'positive', label: '仅正T 低吸高抛' },
                { value: 'reverse', label: '仅反T 高抛低吸' },
              ]}
            />
          </div>
          <div>
            <div style={LABEL}>出场方式</div>
            <Select
              value={cfg.exit_mode}
              onChange={(v: ExitMode) => patch({ exit_mode: v })}
              style={{ width: 150 }}
              options={[
                { value: 'mid_band', label: '中轨止盈（推荐）' },
                { value: 'upper_band', label: '上轨止盈（原版）' },
                { value: 'either', label: '先到先得' },
              ]}
            />
          </div>
          <div style={{ paddingTop: 20 }}>
            <Button type="primary" icon={<PlayCircleOutlined />} loading={loading} onClick={runBacktest}>
              运行回测
            </Button>
          </div>
        </Space>

        {/* ── 高级参数 ── */}
        <div style={{ marginTop: 16, borderTop: '1px solid #26283a', paddingTop: 12 }}>
          <Space size={4}>
            <Button type="link" size="small" style={{ paddingLeft: 0 }} onClick={() => setShowAdvanced(!showAdvanced)}>
              {showAdvanced ? '收起高级参数' : '展开高级参数（17 项）'}
            </Button>
            <Button
              type="link"
              size="small"
              onClick={() => {
                resetConfig()
                setResult(null)
                setScan(null)
              }}
            >
              恢复默认参数
            </Button>
          </Space>

          {showAdvanced && (
            <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fill, minmax(240px, 1fr))', gap: 16, marginTop: 8 }}>
              <div>
                <div style={LABEL}>J 值超卖阈值（警戒激活）</div>
                <Slider min={-20} max={30} step={1} value={cfg.j_oversold} onChange={(v) => patch({ j_oversold: v })} />
                <Text style={{ color: '#6b6d80', fontSize: 11 }}>当前 {cfg.j_oversold}</Text>
              </div>
              <div>
                <div style={LABEL}>J 值超买阈值</div>
                <Slider min={70} max={120} step={1} value={cfg.j_overbought} onChange={(v) => patch({ j_overbought: v })} />
                <Text style={{ color: '#6b6d80', fontSize: 11 }}>当前 {cfg.j_overbought}</Text>
              </div>
              <div>
                <div style={LABEL}>警戒有效窗口（根）</div>
                <Slider min={1} max={20} step={1} value={cfg.signal_window} onChange={(v) => patch({ signal_window: v })} />
                <Text style={{ color: '#6b6d80', fontSize: 11 }}>
                  {cfg.signal_window} 根内等待金叉（J 超卖与金叉几乎不同日，需留窗口）
                </Text>
              </div>
              <div>
                <div style={LABEL}>触轨容差（%）</div>
                <Slider min={0} max={8} step={0.5} value={cfg.touch_tol * 100} onChange={(v) => patch({ touch_tol: v / 100 })} />
                <Text style={{ color: '#6b6d80', fontSize: 11 }}>当前 {(cfg.touch_tol * 100).toFixed(1)}%</Text>
              </div>
              <div>
                <div style={LABEL}>KDJ 周期 N</div>
                <InputNumber min={3} max={30} value={cfg.kdj_n} onChange={(v) => patch({ kdj_n: v ?? 9 })} />
              </div>
              <div>
                <div style={LABEL}>布林周期 N</div>
                <InputNumber min={5} max={60} value={cfg.boll_n} onChange={(v) => patch({ boll_n: v ?? 20 })} />
              </div>
              <div>
                <div style={LABEL}>布林倍数 K</div>
                <InputNumber min={1} max={3} step={0.1} value={cfg.boll_k} onChange={(v) => patch({ boll_k: v ?? 2 })} />
              </div>
              <div>
                <div style={LABEL}>止损比例（%）</div>
                <InputNumber min={1} max={20} step={0.5} value={cfg.stop_loss * 100} onChange={(v) => patch({ stop_loss: (v ?? 5) / 100 })} />
              </div>
              <div>
                <div style={LABEL}>最大持有根数</div>
                <InputNumber min={3} max={120} value={cfg.max_hold} onChange={(v) => patch({ max_hold: v ?? 20 })} />
              </div>
              <div>
                <div style={LABEL}>单次仓位比例（%）</div>
                <InputNumber min={5} max={100} step={5} value={Math.round(cfg.position_ratio * 100)} onChange={(v) => patch({ position_ratio: (v ?? 33) / 100 })} />
              </div>
              <div>
                <div style={LABEL}>往返成本（%）</div>
                <InputNumber min={0} max={1} step={0.01} value={cfg.cost_rate * 100} onChange={(v) => patch({ cost_rate: (v ?? 0.12) / 100 })} />
              </div>
              <div>
                <div style={LABEL}>底仓市值（元）</div>
                <InputNumber min={10000} step={100000} value={cfg.base_capital} onChange={(v) => patch({ base_capital: v ?? 1_000_000 })} style={{ width: 160 }} />
              </div>
              <div>
                <div style={LABEL}>缩量阈值（&lt; 均量 ×）</div>
                <InputNumber min={0.3} max={1.2} step={0.05} value={cfg.volume_ratio} onChange={(v) => patch({ volume_ratio: v ?? 0.85 })} />
              </div>

              {/* ── 分批与动态仓位 ── */}
              <div style={{ gridColumn: '1 / -1', borderTop: '1px dashed #2f3350', paddingTop: 10, marginTop: 4 }}>
                <Text style={{ color: '#4a9eff', fontSize: 12 }}>仓位管理（分批建仓 / 分批止盈 / 动态仓位）</Text>
              </div>
              <div>
                <div style={LABEL}>分批建仓</div>
                <Switch checked={cfg.split_entry} onChange={(v) => patch({ split_entry: v })} />
                <Text style={{ color: '#e8804e', fontSize: 11, marginLeft: 8 }}>
                  权衡项：提盈亏比，但加仓未触发时仓位只有计划的 1/{cfg.max_entries}
                </Text>
              </div>
              <div>
                <div style={LABEL}>最多批次</div>
                <InputNumber min={2} max={5} value={cfg.max_entries} onChange={(v) => patch({ max_entries: v ?? 3 })} disabled={!cfg.split_entry} />
              </div>
              <div>
                <div style={LABEL}>加仓间隔（%）</div>
                <InputNumber min={0.5} max={10} step={0.5} value={cfg.add_step * 100} onChange={(v) => patch({ add_step: (v ?? 2) / 100 })} disabled={!cfg.split_entry} />
              </div>
              <div>
                <div style={LABEL}>分批止盈</div>
                <Switch checked={cfg.split_exit} onChange={(v) => patch({ split_exit: v })} />
                <Text style={{ color: '#6b6d80', fontSize: 11, marginLeft: 8 }}>到中轨先平一部分，剩余等上轨</Text>
              </div>
              <div>
                <div style={LABEL}>首批止盈比例（%）</div>
                <InputNumber min={10} max={90} step={10} value={Math.round(cfg.first_exit_ratio * 100)} onChange={(v) => patch({ first_exit_ratio: (v ?? 50) / 100 })} disabled={!cfg.split_exit} />
              </div>
              <div>
                <div style={LABEL}>按信号强度调仓位</div>
                <Switch checked={cfg.dynamic_position} onChange={(v) => patch({ dynamic_position: v })} />
                <Text style={{ color: '#6b6d80', fontSize: 11, marginLeft: 8 }}>强信号重仓、弱信号轻仓</Text>
              </div>
              <div>
                <div style={LABEL}>仓位下限（%）</div>
                <InputNumber min={5} max={100} step={5} value={Math.round(cfg.min_position_ratio * 100)} onChange={(v) => patch({ min_position_ratio: (v ?? 15) / 100 })} disabled={!cfg.dynamic_position} />
              </div>
              <div>
                <div style={LABEL}>仓位上限（%）</div>
                <InputNumber min={5} max={100} step={5} value={Math.round(cfg.max_position_ratio * 100)} onChange={(v) => patch({ max_position_ratio: (v ?? 50) / 100 })} disabled={!cfg.dynamic_position} />
              </div>
              <div>
                <div style={LABEL}>缩量确认</div>
                <Switch checked={cfg.volume_filter} onChange={(v) => patch({ volume_filter: v })} />
                <Text style={{ color: '#6b6d80', fontSize: 11, marginLeft: 8 }}>开启可提升盈亏比</Text>
              </div>
              <div>
                <div style={LABEL}>仅震荡市做T</div>
                <Switch checked={cfg.trend_filter} onChange={(v) => patch({ trend_filter: v })} />
                <Text style={{ color: '#6b6d80', fontSize: 11, marginLeft: 8 }}>信号稀少但质量高</Text>
              </div>
              <div>
                <div style={LABEL}>背离加分</div>
                <Switch checked={cfg.use_divergence} onChange={(v) => patch({ use_divergence: v })} />
              </div>
            </div>
          )}
        </div>
      </Card>

      {error && (
        <Alert type="error" message={error} style={{ marginTop: 16 }} showIcon closable onClose={() => setError(null)} />
      )}

      {/* ── 结果区 ── */}
      <Tabs
        style={{ marginTop: 16 }}
        activeKey={activeTab}
        onChange={setActiveTab}
        items={[
          {
            key: 'single',
            label: '单组回测',
            children: <SingleResult result={result} loading={loading} />,
          },
          {
            key: 'scan',
            label: '参数对比（A~F 六变体）',
            children: (
              <div>
                <Button icon={<ExperimentOutlined />} loading={scanLoading} onClick={runScan} style={{ marginBottom: 12 }}>
                  运行参数对比
                </Button>
                <ScanTable data={scan} loading={scanLoading} />
              </div>
            ),
          },
        ]}
      />
    </div>
  )
}

/* ══════════════ 单组回测结果 ══════════════ */

function SingleResult({ result, loading }: { result: TBacktestResult | null; loading: boolean }) {
  const chartRef = useRef<HTMLDivElement>(null)
  const inst = useRef<echarts.ECharts | null>(null)

  useEffect(() => {
    if (!chartRef.current) return
    if (!inst.current) inst.current = echarts.init(chartRef.current)
    const curve = result?.equity_curve ?? []
    inst.current.setOption(
      {
        backgroundColor: 'transparent',
        tooltip: { trigger: 'axis' },
        grid: { left: 56, right: 24, top: 30, bottom: 40 },
        xAxis: {
          type: 'category',
          data: curve.map((p) => p.date),
          axisLine: { lineStyle: { color: '#3a3d52' } },
          axisLabel: { color: '#9090a8', fontSize: 11 },
        },
        yAxis: {
          type: 'value',
          name: '累计收益 %',
          nameTextStyle: { color: '#9090a8' },
          axisLine: { lineStyle: { color: '#3a3d52' } },
          axisLabel: { color: '#9090a8', formatter: '{value}%' },
          splitLine: { lineStyle: { color: '#23263a' } },
        },
        series: [
          {
            type: 'line',
            data: curve.map((p) => Number(p.cumulative_pct.toFixed(2))),
            smooth: true,
            symbolSize: 6,
            lineStyle: { width: 2, color: '#4a9eff' },
            itemStyle: { color: '#4a9eff' },
            areaStyle: {
              color: new echarts.graphic.LinearGradient(0, 0, 0, 1, [
                { offset: 0, color: 'rgba(74,158,255,0.28)' },
                { offset: 1, color: 'rgba(74,158,255,0.02)' },
              ]),
            },
          },
        ],
      },
      true,
    )
  }, [result])

  useEffect(() => {
    return () => {
      inst.current?.dispose()
      inst.current = null
    }
  }, [])

  if (loading) {
    return (
      <div style={{ textAlign: 'center', padding: 60 }}>
        <Spin size="large" />
        <div style={{ color: '#9090a8', marginTop: 12 }}>正在回测…</div>
      </div>
    )
  }

  if (!result) {
    return <Alert type="info" showIcon message="填写股票代码后点击「运行回测」" />
  }

  const s = result.stats
  const timeoutRate = s.total_trades > 0 ? (s.timeout_exits / s.total_trades) * 100 : 0

  return (
    <div>
      {result.conclusion && (
        <Alert
          type={s.total_pnl_pct > 0 ? 'success' : s.total_trades === 0 ? 'info' : 'warning'}
          showIcon
          message={<span style={{ whiteSpace: 'pre-wrap' }}>{result.conclusion}</span>}
          style={{ marginBottom: 16 }}
        />
      )}

      <div style={{ display: 'flex', gap: 12, flexWrap: 'wrap' }}>
        <Metric label="交易笔数" value={String(s.total_trades)} sub={`正T ${s.positive_trades} / 反T ${s.reverse_trades}`} />
        <Metric label="胜率" value={`${s.win_rate.toFixed(1)}%`} sub={`${s.win_trades} 胜 / ${s.loss_trades} 负`} />
        <Metric label="单笔平均" value={`${signed(s.avg_pnl_pct)}%`} color={colorOf(s.avg_pnl_pct)} />
        <Metric
          label="累计收益"
          value={`${signed(s.total_pnl_pct)}%`}
          color={colorOf(s.total_pnl_pct)}
          sub={`按 ${(result.config.position_ratio * 100).toFixed(0)}% 仓位`}
        />
        <Metric
          label="盈亏比"
          value={s.profit_factor >= 999 ? '∞' : s.profit_factor.toFixed(2)}
          color={s.profit_factor >= 1.5 ? UP : s.profit_factor >= 1 ? '#e6e6f0' : DOWN}
          sub=">1 为正期望"
        />
        <Metric label="最大回撤" value={`${s.max_drawdown_pct.toFixed(2)}%`} color={DOWN} />
        <Metric label="平均持有" value={`${s.avg_hold_bars.toFixed(1)} 根`} sub={`止损 ${s.stop_loss_exits} / 超时 ${s.timeout_exits}`} />
      </div>

      {timeoutRate > 40 && (
        <Alert
          type="warning"
          showIcon
          style={{ marginTop: 12 }}
          message={`有 ${timeoutRate.toFixed(0)}% 的交易靠「超时强平」离场，说明出场条件过严、多数单子等不到目标位，建议改用「中轨止盈」。`}
        />
      )}

      <Card
        title={<span style={{ color: '#e6e6f0' }}>资金曲线</span>}
        style={{ ...CARD_STYLE, marginTop: 16 }}
        styles={{ header: { ...CARD_STYLE, color: '#e6e6f0' } }}
      >
        <div ref={chartRef} style={{ height: 300 }} />
      </Card>

      <TradeTable trades={result.trades} />
      <SignalList signals={result.signals} />
    </div>
  )
}

/* ══════════════ 交易明细 ══════════════ */

function TradeTable({ trades }: { trades: TTrade[] }) {
  // 按入场日倒序展示（最近的在前），seq 仍保持时间正序，便于对照资金曲线
  const rows = useMemo(() => [...trades].reverse(), [trades])
  if (rows.length === 0) return null
  return (
    <Card
      title={<span style={{ color: '#e6e6f0' }}>交易明细</span>}
      style={{ ...CARD_STYLE, marginTop: 16 }}
      styles={{ header: { ...CARD_STYLE, color: '#e6e6f0' } }}
    >
      <Table<TTrade>
        size="small"
        rowKey="seq"
        dataSource={rows}
        pagination={false}
        scroll={{ x: 900 }}
        columns={[
          { title: '#', dataIndex: 'seq', width: 46 },
          {
            title: '类型',
            dataIndex: 't_type',
            width: 70,
            render: (v: string) => (
              <Tag color={v === '正T' ? 'red' : 'green'} bordered={false}>
                {v}
              </Tag>
            ),
          },
          { title: '入场日', dataIndex: 'entry_date', width: 105 },
          { title: '入场价', dataIndex: 'entry_price', width: 80, render: (v: number) => v.toFixed(2) },
          { title: '出场日', dataIndex: 'exit_date', width: 105 },
          { title: '出场价', dataIndex: 'exit_price', width: 80, render: (v: number) => v.toFixed(2) },
          {
            title: '净收益',
            dataIndex: 'net_pnl_pct',
            width: 90,
            render: (v: number) => <span style={{ color: colorOf(v) }}>{signed(v)}%</span>,
          },
          {
            title: '盈亏(元)',
            dataIndex: 'pnl_amount',
            width: 100,
            render: (v: number) => <span style={{ color: colorOf(v) }}>{signed(v, 0)}</span>,
          },
          { title: '持有', dataIndex: 'hold_bars', width: 60, render: (v: number) => `${v}根` },
          {
            title: '强度',
            dataIndex: 'entry_strength',
            width: 70,
            render: (v: number) => <span style={{ color: '#9090a8' }}>{(v * 100).toFixed(0)}</span>,
          },
          {
            title: '仓位',
            dataIndex: 'used_position_ratio',
            width: 70,
            render: (v: number) => (
              <span style={{ color: '#9090a8' }}>
                {Number.isFinite(v) ? `${(v * 100).toFixed(0)}%` : '—'}
              </span>
            ),
          },
          { title: '出场原因', dataIndex: 'exit_reason', render: (v: string) => <span style={{ color: '#9090a8' }}>{v}</span> },
        ]}
        expandable={{
          // 只在分批（leg 数 > 2）时允许展开，避免无意义的空展开。
          // 用可选链兜底：localStorage 里可能存着加 legs 字段之前的旧回测结果。
          rowExpandable: (r) => (r.legs?.length ?? 0) > 2,
          expandedRowRender: (r) => <LegDetail legs={r.legs ?? []} />,
        }}
      />
    </Card>
  )
}

/** 分批成交明细（展开行） */
function LegDetail({ legs }: { legs: TradeLeg[] }) {
  return (
    <div style={{ padding: '4px 0' }}>
      <div style={{ color: '#6b6d80', fontSize: 11, marginBottom: 6 }}>
        共 {legs.length} 笔成交（买入 {legs.filter((l) => l.action === '买入').length} 批 / 卖出{' '}
        {legs.filter((l) => l.action === '卖出').length} 批）
      </div>
      <Table<TradeLeg>
        size="small"
        rowKey={(_, i) => String(i)}
        dataSource={legs}
        pagination={false}
        columns={[
          { title: '日期', dataIndex: 'date', width: 110 },
          {
            title: '动作',
            dataIndex: 'action',
            width: 70,
            render: (v: string) => (
              <Tag color={v === '买入' ? 'red' : 'green'} bordered={false}>
                {v}
              </Tag>
            ),
          },
          { title: '价格', dataIndex: 'price', width: 80, render: (v: number) => v.toFixed(2) },
          { title: '股数', dataIndex: 'shares', width: 90, render: (v: number) => v.toFixed(0) },
          { title: '金额(元)', dataIndex: 'amount', width: 110, render: (v: number) => v.toFixed(0) },
          { title: '触发原因', dataIndex: 'reason', render: (v: string) => <span style={{ color: '#9090a8' }}>{v}</span> },
        ]}
      />
    </div>
  )
}

/* ══════════════ 信号明细 ══════════════ */

function SignalList({ signals }: { signals: TBacktestResult['signals'] }) {
  // 同样按日期倒序，展示最近发生的信号（正序只会看到最旧的 20 条）
  const rows = useMemo(() => [...signals].reverse().slice(0, 20), [signals])
  if (rows.length === 0) return null
  return (
    <Card
      title={<span style={{ color: '#e6e6f0' }}>信号命中原因（最近 20 条）</span>}
      style={{ ...CARD_STYLE, marginTop: 16 }}
      styles={{ header: { ...CARD_STYLE, color: '#e6e6f0' } }}
    >
      <Space direction="vertical" size={10} style={{ width: '100%' }}>
        {rows.map((sg, i) => (
          <div
            key={`${sg.date}-${i}`}
            style={{ padding: '10px 12px', background: '#1b1e2e', border: '1px solid #26283a', borderRadius: 6 }}
          >
            <Space size={10} wrap>
              <Tag color={sg.direction === 'buy' ? 'red' : 'green'} bordered={false}>
                {sg.direction === 'buy' ? '买入' : '卖出'}
              </Tag>
              <Tag bordered={false} color="blue">
                {sg.t_type}
              </Tag>
              <span style={{ color: '#e6e6f0' }}>{sg.date}</span>
              <span style={{ color: '#9090a8' }}>@ {sg.price.toFixed(2)}</span>
              <span style={{ color: '#6b6d80' }}>J={sg.j_value.toFixed(1)}</span>
              <span style={{ color: '#6b6d80' }}>{sg.boll_shape}</span>
              <span style={{ color: '#6b6d80' }}>强度 {(sg.strength * 100).toFixed(0)}</span>
              {sg.has_divergence && <Tag bordered={false} color="gold">背离</Tag>}
              {sg.volume_shrunk && <Tag bordered={false} color="purple">缩量</Tag>}
            </Space>
            <ul style={{ margin: '8px 0 0', paddingLeft: 20, color: '#9090a8', fontSize: 12 }}>
              {sg.reasons.map((r, j) => (
                <li key={j}>{r}</li>
              ))}
            </ul>
          </div>
        ))}
      </Space>
    </Card>
  )
}

/* ══════════════ 参数对比表 ══════════════ */

function ScanTable({ data, loading }: { data: TScanItem[] | null; loading: boolean }) {
  if (loading) {
    return (
      <div style={{ textAlign: 'center', padding: 60 }}>
        <Spin size="large" />
        <div style={{ color: '#9090a8', marginTop: 12 }}>正在对比六组参数…</div>
      </div>
    )
  }
  if (!data) {
    return <Alert type="info" showIcon message="点击「运行参数对比」横向比较 A~F 六个变体的表现" />
  }

  const best = data.reduce((a, b) => (b.total_pnl_pct > a.total_pnl_pct ? b : a), data[0])

  return (
    <div>
      <Alert
        type="info"
        showIcon
        style={{ marginBottom: 12 }}
        message={
          <span>
            本组标的最优变体：<b style={{ color: UP }}>{best.label}</b>（累计 {signed(best.total_pnl_pct)}%，
            胜率 {best.stats.win_rate.toFixed(1)}%）。注意样本量偏小时结论不稳定，建议多标的验证。
          </span>
        }
      />
      <Table<TScanItem>
        size="small"
        rowKey="label"
        dataSource={data}
        pagination={false}
        rowClassName={(r) => (r.label === best.label ? 'row-best' : '')}
        columns={[
          {
            title: '变体',
            dataIndex: 'label',
            render: (v: string, r) => (
              <span style={{ color: r.label === best.label ? UP : '#e6e6f0' }}>{v}</span>
            ),
          },
          { title: '笔数', dataIndex: ['stats', 'total_trades'], width: 70 },
          {
            title: '胜率',
            width: 90,
            render: (_, r) => <span style={{ color: r.stats.win_rate >= 60 ? UP : '#e6e6f0' }}>{r.stats.win_rate.toFixed(1)}%</span>,
          },
          {
            title: '单笔均',
            width: 90,
            render: (_, r) => <span style={{ color: colorOf(r.stats.avg_pnl_pct) }}>{signed(r.stats.avg_pnl_pct)}%</span>,
          },
          {
            title: '累计',
            width: 100,
            render: (_, r) => <span style={{ color: colorOf(r.total_pnl_pct), fontWeight: 600 }}>{signed(r.total_pnl_pct)}%</span>,
          },
          {
            title: '盈亏比',
            width: 90,
            render: (_, r) => (
              <span style={{ color: r.stats.profit_factor >= 1.5 ? UP : '#e6e6f0' }}>
                {r.stats.profit_factor >= 999 ? '∞' : r.stats.profit_factor.toFixed(2)}
              </span>
            ),
          },
          {
            title: '均持',
            width: 80,
            render: (_, r) => <span style={{ color: '#9090a8' }}>{r.stats.avg_hold_bars.toFixed(1)}</span>,
          },
          {
            title: '超时',
            width: 70,
            render: (_, r) => (
              <span style={{ color: r.stats.timeout_exits > 0 ? DOWN : '#6b6d80' }}>{r.stats.timeout_exits}</span>
            ),
          },
          {
            title: '止损',
            width: 70,
            render: (_, r) => <span style={{ color: r.stats.stop_loss_exits > 0 ? DOWN : '#6b6d80' }}>{r.stats.stop_loss_exits}</span>,
          },
        ]}
      />
    </div>
  )
}
