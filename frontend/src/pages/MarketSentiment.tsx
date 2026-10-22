import { useState, useEffect, useRef, useCallback } from 'react'
import {
  Card,
  Tabs,
  Table,
  Spin,
  Empty,
  Alert,
  Button,
  DatePicker,
  Drawer,
  Tag,
  Tooltip,
  message,
} from 'antd'
import {
  ReloadOutlined,
  DownloadOutlined,
  FireOutlined,
  InfoCircleOutlined,
  DatabaseOutlined,
} from '@ant-design/icons'
import * as echarts from 'echarts'
import {
  api,
  SentimentDaily,
  SentimentArchiveEntry,
  JobRunInfo,
  HotTheme,
  IndustryBoard,
  DragonTigerEntry,
} from '../api'
import { usePersistedState } from '../stores/persistedState'
import dayjs from 'dayjs'

const UP = '#ef4444'
const DOWN = '#22c55e'
const DIM = '#9090a8'
const TEXT = '#e0e0e8'
const SURFACE2 = '#22223a'
const BORDER = '#2a2a42'

function colorNum(val: number): string {
  return val > 0 ? UP : val < 0 ? DOWN : DIM
}

function pct(v: number, signed = true): string {
  return `${signed && v > 0 ? '+' : ''}${v.toFixed(2)}%`
}

/** 后端金额单位为「万元」，此处转换为中文习惯的 万 / 亿 / 万亿 */
function fmtAmount(wan: number | null, absLabel = false): string {
  if (wan === null) return '—'
  const a = Math.abs(wan)
  const sign = absLabel || wan >= 0 ? '' : '-'
  if (a >= 1e8) return `${sign}¥${(a / 1e8).toFixed(2)}万亿`
  if (a >= 1e4) return `${sign}¥${(a / 1e4).toFixed(2)}亿`
  return `${sign}¥${a.toFixed(0)}万`
}

// ── 通用横向条形图 ────────────────────────────────────────────────

interface BarDatum {
  name: string
  value: number
  /** 决定条形颜色与右侧单位文本；缺省时按 value 正负着色 */
  colorBy?: number
  /** 条形末端的文本 */
  label?: string
}

function BarChart({
  data,
  height = 420,
  labelWidth = 96,
}: {
  data: BarDatum[]
  height?: number
  labelWidth?: number
}) {
  const ref = useRef<HTMLDivElement>(null)
  const inst = useRef<echarts.ECharts | null>(null)

  useEffect(() => {
    if (!ref.current) return
    inst.current = echarts.init(ref.current)
    const ro = new ResizeObserver(() => inst.current?.resize())
    ro.observe(ref.current)
    return () => {
      ro.disconnect()
      inst.current?.dispose()
      inst.current = null
    }
  }, [])

  useEffect(() => {
    if (!inst.current) return
    inst.current.setOption(
      {
        grid: { left: labelWidth, right: 92, top: 8, bottom: 8 },
        tooltip: {
          trigger: 'item',
          backgroundColor: SURFACE2,
          borderColor: BORDER,
          textStyle: { color: TEXT, fontSize: 12 },
        },
        xAxis: {
          type: 'value',
          axisLine: { lineStyle: { color: BORDER } },
          axisLabel: { color: DIM, fontSize: 10 },
          splitLine: { lineStyle: { color: BORDER, type: 'dashed' } },
        },
        yAxis: {
          type: 'category',
          inverse: true,
          data: data.map((d) => d.name),
          axisLine: { lineStyle: { color: BORDER } },
          axisTick: { show: false },
          axisLabel: { color: TEXT, fontSize: 11 },
        },
        series: [
          {
            type: 'bar',
            barMaxWidth: 16,
            data: data.map((d) => ({
              value: d.value,
              itemStyle: {
                color: colorNum(d.colorBy ?? d.value),
                borderRadius: 2,
              },
            })),
            label: {
              show: true,
              position: 'right',
              color: DIM,
              fontSize: 10,
              formatter: (p: { dataIndex: number }) =>
                data[p.dataIndex]?.label ?? '',
            },
          },
        ],
      },
      true,
    )
  }, [data, labelWidth])

  return <div ref={ref} style={{ width: '100%', height }} />
}

// ── KPI 卡片 ──────────────────────────────────────────────────────

function Kpi({
  label,
  value,
  sub,
  accent,
}: {
  label: string
  value: React.ReactNode
  sub?: React.ReactNode
  accent?: string
}) {
  return (
    <Card size="small" style={{ flex: 1, minWidth: 178, position: 'relative', overflow: 'hidden' }}>
      <div
        style={{
          position: 'absolute',
          left: 0,
          top: 0,
          bottom: 0,
          width: 3,
          background: accent ?? 'var(--primary)',
        }}
      />
      <div style={{ color: 'var(--text-dim)', fontSize: 12 }}>{label}</div>
      <div style={{ fontSize: 20, fontWeight: 700, margin: '3px 0' }}>{value}</div>
      {sub && (
        <div style={{ color: 'var(--text-dim)', fontSize: 11, lineHeight: 1.5 }}>{sub}</div>
      )}
    </Card>
  )
}

// ── 主页面 ────────────────────────────────────────────────────────

export default function MarketSentimentPage() {
  const [data, setData] = useState<SentimentDaily | null>(null)
  const [loading, setLoading] = useState(false)
  const [date, setDate] = usePersistedState<string>('sentiment.date', '')
  const [tab, setTab] = usePersistedState<string>('sentiment.tab', 'lhb')
  // 已归档日期。没有它用户只能盲猜哪天生成过，猜错就是一次 20 秒冷启动。
  const [archive, setArchive] = useState<SentimentArchiveEntry[]>([])
  const [archiveOpen, setArchiveOpen] = useState(false)
  const [archiveLoading, setArchiveLoading] = useState(false)
  // 自动归档最近一次运行。无人值守的定时任务最怕悄悄坏了没人知道，失败要能看见。
  const [lastRun, setLastRun] = useState<JobRunInfo | null>(null)

  const loadArchive = useCallback(async () => {
    setArchiveLoading(true)
    try {
      const r = await api.listSentimentDates()
      setArchive(r.entries)
      setLastRun(r.last_run)
    } catch {
      // 目录拉不到只是少了引导，不该把整页打断，静默降级即可
    }
    setArchiveLoading(false)
  }, [])

  const load = useCallback(
    async (d?: string, refresh = false) => {
      setLoading(true)
      try {
        const r = await api.getSentimentDaily(d || undefined, refresh)
        setData(r)
        // 首次生成某天后会多出一条归档，目录要跟上
        void loadArchive()
      } catch (e) {
        message.error(e instanceof Error ? e.message : '加载情绪日报失败')
      }
      setLoading(false)
    },
    [loadArchive],
  )

  useEffect(() => {
    load(date)
    void loadArchive()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  function handleDateChange(d: dayjs.Dayjs | null) {
    const ds = d ? d.format('YYYY-MM-DD') : ''
    setDate(ds)
    load(ds)
  }

  function handleExport() {
    window.open(api.sentimentExportUrl(date || undefined), '_blank')
    // 导出后服务端会留存一份，稍后刷新目录让「下载 HTML」亮起来
    setTimeout(() => void loadArchive(), 1200)
  }

  const archivedDates = new Set(archive.map((e) => e.date))
  const currentArchived = data ? archivedDates.has(data.date) : false

  if (loading && !data) {
    return (
      <div style={{ textAlign: 'center', padding: '90px 0' }}>
        <Spin size="large" />
        <div style={{ color: 'var(--text-dim)', marginTop: 16, fontSize: 13 }}>
          {date && !archivedDates.has(date)
            ? '正在首次生成该交易日的日报……'
            : '正在加载情绪日报……'}
        </div>
        <div style={{ color: 'var(--text-dim)', marginTop: 6, fontSize: 12 }}>
          {/* 实测冷启动约 20 秒（逐只抓全市场 5,600+ 只日线）；已归档的交易日约 0.006s 直接读库。
              过去的文案写「30–60 秒」，与实测不符，按实测值给，避免用户误判卡死。 */}
          {date && !archivedDates.has(date)
            ? '需逐只抓取全市场成分股日线，约需 20 秒；生成后自动归档，下次打开即时呈现'
            : '该日已归档，正在读取归档结果'}
        </div>
      </div>
    )
  }

  if (!data) return <Empty description="暂无数据" />

  const { lhb, themes, boards, breadth, methodology } = data
  const topTheme: HotTheme | undefined = themes.themes[0]
  const topBoard: IndustryBoard | undefined = boards[0]
  const botBoard: IndustryBoard | undefined = boards[boards.length - 1]

  // ── 龙虎榜 ──
  const lhbChart: BarDatum[] = lhb.entries.slice(0, 15).map((e) => ({
    name: e.name,
    value: e.net_amount,
    colorBy: e.net_amount,
    label: pct(e.pct_change),
  }))

  const lhbColumns = [
    { title: '#', key: 'i', width: 46, render: (_: unknown, __: unknown, i: number) => i + 1 },
    { title: '代码', dataIndex: 'code', key: 'code', width: 82, render: (v: string) => <span className="dim">{v}</span> },
    {
      title: '名称',
      dataIndex: 'name',
      key: 'name',
      width: 100,
      render: (v: string, r: DragonTigerEntry) => (
        <span style={{ fontWeight: 600 }}>
          {v}
          {lhb.entries.filter((x) => x.code === r.code).length > 1 && (
            <Tooltip title="该股触发多个上榜原因，东财返回多条记录">
              <sup style={{ color: 'var(--primary)', fontSize: 9, marginLeft: 2 }}>重</sup>
            </Tooltip>
          )}
        </span>
      ),
    },
    {
      title: '涨跌幅',
      dataIndex: 'pct_change',
      key: 'pct_change',
      align: 'right' as const,
      render: (v: number) => <span style={{ color: colorNum(v) }}>{pct(v)}</span>,
    },
    {
      title: '净买入',
      dataIndex: 'net_amount',
      key: 'net_amount',
      align: 'right' as const,
      sorter: (a: DragonTigerEntry, b: DragonTigerEntry) => a.net_amount - b.net_amount,
      defaultSortOrder: 'descend' as const,
      render: (v: number) => (
        <span style={{ color: colorNum(v), fontWeight: 600 }}>{fmtAmount(v)}</span>
      ),
    },
    {
      title: '换手率',
      dataIndex: 'turnover',
      key: 'turnover',
      align: 'right' as const,
      render: (v: number) => `${v.toFixed(2)}%`,
    },
    {
      title: '成交额',
      dataIndex: 'amount',
      key: 'amount',
      align: 'right' as const,
      render: (v: number) => <span className="dim">{fmtAmount(v)}</span>,
    },
    {
      title: '上榜原因',
      dataIndex: 'reason',
      key: 'reason',
      render: (v: string) => <span style={{ color: 'var(--text-dim)', fontSize: 11 }}>{v}</span>,
    },
  ]

  // ── 题材 ──
  const themeChart: BarDatum[] = themes.themes.slice(0, 20).map((t) => ({
    name: t.tag,
    value: t.count,
    colorBy: t.avg_pct,
    label: `均${pct(t.avg_pct)}`,
  }))

  // 强势股去重（同一只可能归属多个题材）
  const hotStocks = (() => {
    const seen = new Map<string, { code: string; name: string; pct_change: number; reason: string }>()
    for (const t of themes.themes) {
      for (const s of t.stocks) {
        if (!seen.has(s.code)) seen.set(s.code, s)
      }
    }
    return [...seen.values()].sort((a, b) => b.pct_change - a.pct_change)
  })()

  // ── 行业 ──
  const boardTop: BarDatum[] = boards.slice(0, 12).map((b) => ({
    name: b.name,
    value: b.change_pct,
    colorBy: b.change_pct,
    label: `${b.up_count}涨/${b.down_count}跌`,
  }))
  const boardBot: BarDatum[] = boards
    .slice(Math.max(0, boards.length - 12))
    .map((b) => ({
      name: b.name,
      value: b.change_pct,
      colorBy: b.change_pct,
      label: `${b.up_count}涨/${b.down_count}跌`,
    }))

  const boardColumns = [
    { title: '#', key: 'i', width: 46, render: (_: unknown, __: unknown, i: number) => i + 1 },
    { title: '行业板块', dataIndex: 'name', key: 'name', width: 110 },
    {
      title: '涨跌幅',
      dataIndex: 'change_pct',
      key: 'change_pct',
      align: 'right' as const,
      sorter: (a: IndustryBoard, b: IndustryBoard) => a.change_pct - b.change_pct,
      defaultSortOrder: 'descend' as const,
      render: (v: number) => <span style={{ color: colorNum(v), fontWeight: 600 }}>{pct(v)}</span>,
    },
    {
      title: '上涨',
      dataIndex: 'up_count',
      key: 'up_count',
      align: 'right' as const,
      render: (v: number) => <span style={{ color: UP }}>{v}</span>,
    },
    {
      title: '下跌',
      dataIndex: 'down_count',
      key: 'down_count',
      align: 'right' as const,
      render: (v: number) => <span style={{ color: DOWN }}>{v}</span>,
    },
    { title: '平盘', dataIndex: 'flat_count', key: 'flat_count', align: 'right' as const },
    { title: '成分(东财)', dataIndex: 'member_count', key: 'member_count', align: 'right' as const },
    {
      title: '计算家数',
      dataIndex: 'calc_count',
      key: 'calc_count',
      align: 'right' as const,
      render: (v: number) => <span className="dim">{v}</span>,
    },
    {
      title: '成交额',
      dataIndex: 'amount',
      key: 'amount',
      align: 'right' as const,
      render: (v: number | null) => <span className="dim">{fmtAmount(v)}</span>,
    },
    { title: '领涨股', dataIndex: 'leader', key: 'leader', width: 92 },
    {
      title: '领涨涨幅',
      dataIndex: 'leader_pct',
      key: 'leader_pct',
      align: 'right' as const,
      render: (v: number) => <span style={{ color: colorNum(v) }}>{pct(v)}</span>,
    },
  ]

  const tabs = [
    {
      key: 'lhb',
      label: `龙虎榜(${lhb.stock_count}家/${lhb.record_count}条)`,
      children: (
        <div>
          {lhb.record_count > lhb.stock_count && (
            <Alert
              type="info"
              showIcon
              icon={<InfoCircleOutlined />}
              style={{ marginBottom: 12 }}
              message={
                <span style={{ fontSize: 12 }}>
                  净买入合计 <b>{fmtAmount(lhb.net_total)}</b> 为按{' '}
                  <b>{lhb.stock_count}</b> 只个股去重后的结果（同一只触发多个上榜原因时取最大净买入额）；
                  {lhb.record_count} 条记录直接求和为 <b>{fmtAmount(lhb.records_sum)}</b>。
                  下表列出全部记录，重复个股以 <sup style={{ color: 'var(--primary)' }}>重</sup> 标记。
                </span>
              }
            />
          )}
          <Card size="small" title="龙虎榜净买入 TOP 15" style={{ marginBottom: 12 }}>
            {lhbChart.length > 0 ? (
              <BarChart data={lhbChart} height={400} labelWidth={80} />
            ) : (
              <Empty description="当日无龙虎榜数据" />
            )}
          </Card>
          <Card size="small" title={`全市场龙虎榜明细（${lhb.record_count} 条）`}>
            <Table
              rowKey={(r) => `${r.code}-${r.reason}`}
              columns={lhbColumns}
              dataSource={lhb.entries}
              size="small"
              scroll={{ y: 560 }}
              pagination={false}
            />
          </Card>
        </div>
      ),
    },
    {
      key: 'theme',
      label: `题材热点(${themes.stock_count}只/${themes.theme_total}题材)`,
      children: (
        <div>
          <Card
            size="small"
            title="题材热度 TOP 20（同花顺 reason 标签词频）"
            extra={<span style={{ color: 'var(--text-dim)', fontSize: 11 }}>条长=出现只数 · 颜色=题材平均涨跌幅</span>}
            style={{ marginBottom: 12 }}
          >
            {themeChart.length > 0 ? (
              <BarChart data={themeChart} height={520} labelWidth={110} />
            ) : (
              <Empty description="当日无题材数据" />
            )}
          </Card>
          <Card size="small" title={`当日强势股明细（${hotStocks.length} 只，按涨幅降序）`}>
            <Table
              rowKey="code"
              size="small"
              scroll={{ y: 520 }}
              pagination={false}
              dataSource={hotStocks}
              columns={[
                { title: '#', key: 'i', width: 46, render: (_: unknown, __: unknown, i: number) => i + 1 },
                {
                  title: '代码',
                  dataIndex: 'code',
                  key: 'code',
                  width: 82,
                  render: (v: string) => <span className="dim">{v}</span>,
                },
                { title: '名称', dataIndex: 'name', key: 'name', width: 100, render: (v: string) => <b>{v}</b> },
                {
                  title: '涨幅',
                  dataIndex: 'pct_change',
                  key: 'pct_change',
                  align: 'right' as const,
                  sorter: (a: { pct_change: number }, b: { pct_change: number }) => a.pct_change - b.pct_change,
                  defaultSortOrder: 'descend' as const,
                  render: (v: number) => <span style={{ color: colorNum(v), fontWeight: 600 }}>{pct(v)}</span>,
                },
                {
                  title: '题材归因',
                  dataIndex: 'reason',
                  key: 'reason',
                  render: (v: string) => (
                    <span>
                      {v
                        .split('+')
                        .filter((t) => t.trim())
                        .map((t) => (
                          <Tag key={t} style={{ fontSize: 10.5, marginBottom: 2 }}>
                            {t.trim()}
                          </Tag>
                        ))}
                    </span>
                  ),
                },
              ]}
            />
          </Card>
        </div>
      ),
    },
    {
      key: 'board',
      label: `行业轮动(${boards.length}个行业)`,
      children: (
        <div>
          <Card size="small" title="行业涨跌幅榜 · 领涨 TOP 12" style={{ marginBottom: 12 }}>
            <BarChart data={boardTop} height={340} labelWidth={92} />
          </Card>
          <Card size="small" title="行业涨跌幅榜 · 领跌 TOP 12" style={{ marginBottom: 12 }}>
            <BarChart data={boardBot} height={340} labelWidth={92} />
          </Card>
          <Card size="small" title={`行业板块全景（${boards.length} 个行业）`}>
            <Table
              rowKey="name"
              columns={boardColumns}
              dataSource={boards}
              size="small"
              scroll={{ y: 560 }}
              pagination={false}
            />
          </Card>
        </div>
      ),
    },
  ]

  return (
    <div>
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          gap: 12,
          marginBottom: 16,
          flexWrap: 'wrap',
        }}
      >
        <h2 style={{ margin: 0 }}>市场情绪日报</h2>
        <DatePicker
          value={date ? dayjs(date) : null}
          format="YYYY-MM-DD"
          onChange={handleDateChange}
          placeholder="选择交易日"
          style={{ width: 150 }}
          allowClear
          // 该版本 antd 的 current 是 `string | number | Dayjs` 联合类型，
          // 统一交给 dayjs 归一后再取日期，避免逐类型分支。
          cellRender={(current, info) => {
            if (info.type !== 'date') return info.originNode
            const d = dayjs(current)
            const ds = d.format('YYYY-MM-DD')
            if (!archivedDates.has(ds)) return info.originNode
            return (
              <div
                title={`${ds} 已归档，可秒开`}
                style={{
                  borderRadius: 4,
                  background: 'rgba(232,128,78,.20)',
                  color: 'var(--primary-light)',
                  fontWeight: 600,
                }}
              >
                {d.date()}
              </div>
            )
          }}
        />
        <Button icon={<DatabaseOutlined />} onClick={() => setArchiveOpen(true)}>
          归档 ({archive.length})
        </Button>
        <Button icon={<ReloadOutlined />} onClick={() => load(date, true)} loading={loading}>
          重新计算
        </Button>
        <Button icon={<DownloadOutlined />} onClick={handleExport}>
          导出 HTML
        </Button>
        <span style={{ color: 'var(--text-dim)', fontSize: 12 }}>
          {data.date}
          {methodology.from_cache
            ? ' · 读取自缓存'
            : ` · 计算耗时 ${(methodology.elapsed_ms / 1000).toFixed(1)}s`}
          {currentArchived ? ' · 已归档' : ''}
        </span>
      </div>

      <Drawer
        title={
          <span>
            已归档交易日
            <span style={{ color: 'var(--text-dim)', fontSize: 12, fontWeight: 400, marginLeft: 8 }}>
              共 {archive.length} 天 · 点击直接打开
            </span>
          </span>
        }
        placement="right"
        width={520}
        open={archiveOpen}
        onClose={() => setArchiveOpen(false)}
        styles={{ body: { padding: 0 } }}
      >
        {lastRun && (
          <div
            style={{
              padding: '10px 16px',
              borderBottom: '1px solid var(--border)',
              background: lastRun.outcome === 'failed' ? 'rgba(239,68,68,.08)' : 'var(--bg-surface)',
              fontSize: 11.5,
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: 6, marginBottom: 3 }}>
              <span
                style={{
                  width: 7,
                  height: 7,
                  borderRadius: '50%',
                  flexShrink: 0,
                  background:
                    lastRun.outcome === 'failed'
                      ? 'var(--up)'
                      : lastRun.outcome === 'succeeded'
                        ? 'var(--down)'
                        : 'var(--text-dim)',
                }}
              />
              <span style={{ fontWeight: 600 }}>自动归档</span>
              <span style={{ color: 'var(--text-dim)' }}>
                {dayjs(lastRun.started_at * 1000).format('MM-DD HH:mm')}
                {lastRun.elapsed_ms != null &&
                  ` · ${(lastRun.elapsed_ms / 1000).toFixed(1)}s`}
              </span>
            </div>
            <div
              style={{
                color: lastRun.outcome === 'failed' ? 'var(--up)' : 'var(--text-dim)',
                // 失败时 detail 是排障的第一手线索，换行不能吞
                whiteSpace: 'pre-wrap',
              }}
            >
              {lastRun.detail ?? '—'}
            </div>
          </div>
        )}
        {archiveLoading && archive.length === 0 ? (
          <div style={{ textAlign: 'center', padding: '50px 0' }}>
            <Spin />
          </div>
        ) : archive.length === 0 ? (
          <Empty
            description="还没有归档任何交易日"
            style={{ marginTop: 70 }}
            image={Empty.PRESENTED_IMAGE_SIMPLE}
          />
        ) : (
          archive.map((e) => {
            const active = data?.date === e.date
            return (
              <div
                key={e.date}
                onClick={() => {
                  setDate(e.date)
                  setArchiveOpen(false)
                  load(e.date)
                }}
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: 12,
                  padding: '12px 16px',
                  cursor: 'pointer',
                  borderBottom: '1px solid var(--border)',
                  background: active ? 'rgba(232,128,78,.09)' : undefined,
                  borderLeft: active ? '3px solid var(--primary)' : '3px solid transparent',
                }}
                onMouseEnter={(ev) => {
                  if (!active) ev.currentTarget.style.background = 'var(--bg-surface)'
                }}
                onMouseLeave={(ev) => {
                  if (!active) ev.currentTarget.style.background = ''
                }}
              >
                <span
                  style={{
                    width: 7,
                    height: 7,
                    borderRadius: '50%',
                    background: active ? 'var(--primary)' : 'var(--down)',
                    flexShrink: 0,
                  }}
                />
                <span style={{ width: 92, fontWeight: 600, flexShrink: 0 }}>
                  {e.date}
                  {active && <span style={{ color: 'var(--primary)', fontSize: 11 }}> 当前</span>}
                </span>
                <span style={{ flex: 1, color: 'var(--text-dim)', fontSize: 11.5 }}>
                  {dayjs(e.generated_at * 1000).format('MM-DD HH:mm')} 归档 · 覆盖{' '}
                  {e.covered_count.toLocaleString()} 只 · {e.board_count} 行业 ·{' '}
                  {(e.payload_bytes / 1024).toFixed(0)} KB
                </span>
                <span style={{ display: 'flex', gap: 6, flexShrink: 0 }}>
                  <Button
                    size="small"
                    onClick={(ev) => {
                      ev.stopPropagation()
                      window.open(api.sentimentExportUrl(e.date), '_blank')
                      setTimeout(() => void loadArchive(), 1200)
                    }}
                  >
                    {e.has_export ? '下载 HTML' : '导出 HTML'}
                  </Button>
                </span>
              </div>
            )
          })
        )}
      </Drawer>

      <div style={{ display: 'flex', gap: 10, marginBottom: 16, flexWrap: 'wrap' }}>
        <Kpi
          label="龙虎榜上榜家数"
          value={
            <>
              {lhb.stock_count}
              <span style={{ fontSize: 12, fontWeight: 400, color: 'var(--text-dim)' }}> 家</span>
            </>
          }
          sub={`${lhb.record_count} 条记录 · 净买入 ${lhb.net_buy_count} 家 / 净卖出 ${lhb.net_sell_count} 家`}
        />
        <Kpi
          label="龙虎榜净买入合计"
          accent={UP}
          value={<span style={{ color: colorNum(lhb.net_total) }}>{fmtAmount(lhb.net_total)}</span>}
          sub={`买入方 ${fmtAmount(lhb.buy_total)} · 卖出方 ${fmtAmount(lhb.sell_total, true)}`}
        />
        <Kpi
          label="最热题材"
          accent="#f9c74f"
          value={
            <span style={{ fontSize: 16 }}>
              <FireOutlined style={{ color: 'var(--primary)', marginRight: 4 }} />
              {topTheme?.tag ?? '—'}
            </span>
          }
          sub={
            topTheme
              ? `${topTheme.count} 只共振 · 领涨 ${topTheme.leader ?? '—'} ${pct(topTheme.leader_pct)}`
              : '当日无题材数据'
          }
        />
        <Kpi
          label="领涨 / 领跌行业"
          accent={DOWN}
          value={
            <span style={{ fontSize: 14, lineHeight: 1.45 }}>
              {topBoard ? (
                <>
                  {topBoard.name}{' '}
                  <span style={{ color: colorNum(topBoard.change_pct) }}>
                    {pct(topBoard.change_pct)}
                  </span>
                </>
              ) : (
                '—'
              )}
              <br />
              {botBoard ? (
                <>
                  {botBoard.name}{' '}
                  <span style={{ color: colorNum(botBoard.change_pct) }}>
                    {pct(botBoard.change_pct)}
                  </span>
                </>
              ) : (
                '—'
              )}
            </span>
          }
          sub={`全市场 ${boards.length} 个行业参与统计`}
        />
        <Kpi
          label="全市场涨跌家数"
          accent="#7c8cff"
          value={
            <>
              <span style={{ color: UP }}>{breadth.up_count}</span>
              <span style={{ color: 'var(--text-dim)' }}> / </span>
              <span style={{ color: DOWN }}>{breadth.down_count}</span>
            </>
          }
          sub={`覆盖 ${breadth.covered_count} 只 · 平盘 ${breadth.flat_count} · 成交额 ${fmtAmount(breadth.total_amount)}`}
        />
      </div>

      {breadth.indices.length > 0 && (
        <div style={{ display: 'flex', gap: 8, marginBottom: 16, flexWrap: 'wrap' }}>
          {breadth.indices.map((ix) => (
            <div
              key={ix.code}
              style={{
                background: 'var(--bg-surface)',
                border: '1px solid var(--border)',
                borderRadius: 6,
                padding: '6px 10px',
                display: 'flex',
                gap: 8,
                alignItems: 'baseline',
              }}
            >
              <span style={{ color: 'var(--text-dim)', fontSize: 11 }}>{ix.name}</span>
              <span style={{ fontSize: 13, fontWeight: 600, color: colorNum(ix.pct_change) }}>
                {ix.close.toFixed(2)}
              </span>
              <span style={{ fontSize: 11, color: colorNum(ix.pct_change) }}>
                {pct(ix.pct_change)}
              </span>
            </div>
          ))}
        </div>
      )}

      <Tabs activeKey={tab} onChange={setTab} items={tabs} size="small" />

      <div
        style={{
          marginTop: 20,
          paddingTop: 12,
          borderTop: '1px solid var(--border)',
          color: 'var(--text-dim)',
          fontSize: 11,
          lineHeight: 1.8,
        }}
      >
        <div>
          <b style={{ color: '#b9b9d0' }}>数据来源</b>
          <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap', margin: '6px 0' }}>
            {data.sources.map((s, i) => (
              <span
                key={s}
                style={{
                  background: 'var(--bg-surface)',
                  border: '1px solid var(--border)',
                  borderRadius: 4,
                  padding: '2px 7px',
                }}
              >
                {['①', '②', '③', '④', '⑤', '⑥', '⑦', '⑧'][i] ?? '·'} {s}
              </span>
            ))}
          </div>
        </div>
        <Alert
          type="warning"
          showIcon
          style={{ marginTop: 8 }}
          message={
            <div style={{ fontSize: 11, lineHeight: 1.7 }}>
              <b>方法学说明</b>：行业涨跌幅为<b>{methodology.board_method}</b>
              {methodology.board_method.includes('官方')
                ? '，成交额与涨跌家数同样取自东财官方板块快照；市场宽度来自东财全市场快表，该快表仅覆盖沪深两市、不含北交所，与历史交易日的逐股聚合口径不同，两者不可直接横向比较。'
                : '，即该行业成分股当日涨跌幅的算术平均，并非东财板块指数的官方涨跌幅——东财未提供按历史日期查询板块指数的公开接口。'}
              「成分(东财)」列为东财官方披露的板块成分股家数。
              全市场涨跌家数覆盖 {breadth.covered_count} 只个股，停牌及当日无成交个股未计入。
              {methodology.notes.map((n, i) => (
                <div key={i}>· {n}</div>
              ))}
            </div>
          }
        />
      </div>
    </div>
  )
}
