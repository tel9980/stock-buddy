import { useState, useEffect, useRef } from 'react'
import { Card, Table, Input, Button, Tag, Spin, Empty, message, Descriptions } from 'antd'
import { SearchOutlined } from '@ant-design/icons'
import * as echarts from 'echarts'
import { api, FinancialSummary, FinancialIndicator } from '../api'

function colorYoY(val: number | null): string {
  if (val == null) return '#999'
  return val > 0 ? '#cf1322' : val < 0 ? '#3f8600' : '#666'
}

export default function FinancialAnalysisPage() {
  const [code, setCode] = useState('')
  const [data, setData] = useState<FinancialSummary | null>(null)
  const [loading, setLoading] = useState(false)
  const chartRef = useRef<HTMLDivElement>(null)

  async function search() {
    if (!code) return
    setLoading(true)
    try {
      const d = await api.getFinancialIndicators(code)
      setData(d)
      setTimeout(() => renderChart(d.indicators), 150)
    } catch {
      message.error('查询财务数据失败')
    }
    setLoading(false)
  }

  function renderChart(indicators: FinancialIndicator[]) {
    if (!chartRef.current || indicators.length < 2) return
    const reversed = [...indicators].reverse()
    const chart = echarts.init(chartRef.current)
    chart.setOption({
      tooltip: { trigger: 'axis' },
      legend: { top: 0, textStyle: { color: '#999', fontSize: 11 } },
      grid: { left: 80, right: 20, top: 35, bottom: 30 },
      xAxis: { type: 'category', data: reversed.map(d => d.report_date.slice(0, 7)), axisLabel: { color: '#999', fontSize: 10 } },
      yAxis: [
        { type: 'value', name: '万元', axisLabel: { color: '#999', formatter: (v: number) => (v / 10000).toFixed(0) + '亿' } },
        { type: 'value', name: '%', axisLabel: { color: '#999' } },
      ],
      series: [
        { name: '营业收入', type: 'bar', data: reversed.map(d => d.revenue), yAxisIndex: 0, itemStyle: { color: '#e8804e' } },
        { name: '归母净利润', type: 'bar', data: reversed.map(d => d.net_profit), yAxisIndex: 0, itemStyle: { color: '#cf1322' } },
        { name: 'ROE', type: 'line', data: reversed.map(d => d.roe), yAxisIndex: 1, lineStyle: { color: '#1890ff' } },
      ],
    })
    return () => chart.dispose()
  }

  function valDot(v: number | null): string {
    return v != null ? v.toFixed(2) : '--'
  }

  return (
    <div>
      <h2 style={{ margin: '0 0 16px' }}>财务分析</h2>

      <div style={{ display: 'flex', gap: 8, marginBottom: 16 }}>
        <Input placeholder="输入股票代码" value={code} onChange={e => setCode(e.target.value)} style={{ width: 160 }} />
        <Button icon={<SearchOutlined />} onClick={search} loading={loading}>查询</Button>
      </div>

      {loading && <Spin size="large" style={{ display: 'block', margin: '60px auto' }} />}

      {data && !loading && (
        <>
          <Descriptions bordered size="small" column={4} style={{ marginBottom: 16 }}>
            <Descriptions.Item label="股票名称">{data.stock_name}</Descriptions.Item>
            <Descriptions.Item label="最新报告期">{data.latest_report_date}</Descriptions.Item>
            <Descriptions.Item label="PE">{data.pe?.toFixed(2) ?? '--'}</Descriptions.Item>
            <Descriptions.Item label="PB">{data.pb?.toFixed(2) ?? '--'}</Descriptions.Item>
          </Descriptions>

          <Card size="small" style={{ marginBottom: 16 }}>
            <div ref={chartRef} style={{ height: 280 }} />
          </Card>

          <Table
            dataSource={data.indicators}
            rowKey="report_date"
            size="small"
            scroll={{ x: 1300 }}
            columns={[
              { title: '报告期', dataIndex: 'report_date', width: 100, fixed: 'left' as const },
              { title: '每股收益', dataIndex: 'eps', width: 80, render: (v: number | null) => valDot(v) },
              { title: '每股净资产', dataIndex: 'bvps', width: 90, render: (v: number | null) => valDot(v) },
              { title: 'ROE(%)', dataIndex: 'roe', width: 80, render: (v: number | null) => valDot(v) },
              { title: 'ROA(%)', dataIndex: 'roa', width: 80, render: (v: number | null) => valDot(v) },
              { title: '毛利率(%)', dataIndex: 'gross_margin', width: 90, render: (v: number | null) => valDot(v) },
              { title: '净利率(%)', dataIndex: 'net_margin', width: 90, render: (v: number | null) => valDot(v) },
              {
                title: '营收(万)', dataIndex: 'revenue', width: 120,
                render: (v: number | null) => v != null ? (v / 10000).toFixed(2) + '亿' : '--',
              },
              { title: '营收同比(%)', dataIndex: 'revenue_yoy', width: 100, render: (v: number | null) => <span style={{ color: colorYoY(v) }}>{valDot(v)}</span> },
              {
                title: '归母净利(万)', dataIndex: 'net_profit', width: 130,
                render: (v: number | null) => v != null ? (v / 10000).toFixed(2) + '亿' : '--',
              },
              { title: '净利同比(%)', dataIndex: 'net_profit_yoy', width: 100, render: (v: number | null) => <span style={{ color: colorYoY(v) }}>{valDot(v)}</span> },
              {
                title: '扣非净利(万)', dataIndex: 'deducted_profit', width: 130,
                render: (v: number | null) => v != null ? (v / 10000).toFixed(2) + '亿' : '--',
              },
              { title: '资产负债率(%)', dataIndex: 'debt_ratio', width: 110, render: (v: number | null) => valDot(v) },
              { title: '流动比率', dataIndex: 'current_ratio', width: 80, render: (v: number | null) => valDot(v) },
              { title: '速动比率', dataIndex: 'quick_ratio', width: 80, render: (v: number | null) => valDot(v) },
            ]}
          />
        </>
      )}

      {!data && !loading && <Empty description="请输入股票代码查询财务数据" />}
    </div>
  )
}
