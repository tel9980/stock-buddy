import ReactECharts from 'echarts-for-react';
import { Kline, TradeSignal } from '../api';

interface KLineChartProps {
  klines: Kline[];
  signals?: TradeSignal[];
  height?: number;
}

export default function KLineChart({ klines, signals = [], height = 500 }: KLineChartProps) {
  if (!klines || klines.length === 0) {
    return <div className="chart-loading">暂无K线数据</div>;
  }

  const dates = klines.map(k => k.date);
  const ohlc = klines.map(k => [k.open, k.close, k.low, k.high]);
  const volumes = klines.map(k => k.volume);

  // 生成买卖点标记数据
  const buyMarkers: any[] = [];
  const sellMarkers: any[] = [];

  for (const sig of signals) {
    const idx = dates.indexOf(sig.date);
    if (idx === -1) continue;
    const kline = klines[idx];
    const marker = {
      name: sig.source,
      value: sig.description,
      coord: [sig.date, sig.signal_type === 'Buy' ? kline.low * 0.995 : kline.high * 1.005],
      symbol: sig.signal_type === 'Buy' ? 'pin' : 'pin',
      symbolSize: 38,
      symbolRotate: sig.signal_type === 'Buy' ? 0 : 180,
      itemStyle: {
        color: sig.signal_type === 'Buy' ? '#ef4444' : '#22c55e',
        borderColor: sig.signal_type === 'Buy' ? '#ff6b6b' : '#51cf66',
        borderWidth: 1,
      },
      label: {
        show: true,
        position: sig.signal_type === 'Buy' ? 'bottom' : 'top',
        formatter: sig.signal_type === 'Buy' ? '买' : '卖',
        color: '#fff',
        fontSize: 10,
        fontWeight: 'bold',
      },
    };
    if (sig.signal_type === 'Buy') {
      buyMarkers.push(marker);
    } else {
      sellMarkers.push(marker);
    }
  }

  const allMarkers = [...buyMarkers, ...sellMarkers];

  const option = {
    backgroundColor: 'transparent',
    animation: false,
    legend: {
      show: false,
    },
    tooltip: {
      trigger: 'axis',
      axisPointer: { type: 'cross' },
      backgroundColor: 'rgba(26, 26, 46, 0.95)',
      borderColor: '#2a2a42',
      textStyle: { color: '#e0e0e8' },
    },
    axisPointer: {
      link: [{ xAxisIndex: 'all' }],
    },
    grid: [
      { left: '8%', right: '4%', top: '4%', height: '62%' },
      { left: '8%', right: '4%', top: '72%', height: '22%' },
    ],
    xAxis: [
      {
        type: 'category',
        data: dates,
        scale: true,
        boundaryGap: false,
        axisLine: { lineStyle: { color: '#2a2a42' } },
        axisLabel: { color: '#9090a8', fontSize: 11 },
        splitLine: { show: false },
      },
      {
        type: 'category',
        gridIndex: 1,
        data: dates,
        scale: true,
        boundaryGap: false,
        axisLine: { lineStyle: { color: '#2a2a42' } },
        axisLabel: { show: false },
        splitLine: { show: false },
      },
    ],
    yAxis: [
      {
        scale: true,
        splitLine: { lineStyle: { color: '#2a2a42' } },
        axisLabel: { color: '#9090a8', fontSize: 11 },
      },
      {
        gridIndex: 1,
        splitNumber: 2,
        axisLabel: { show: false },
        axisLine: { show: false },
        splitLine: { show: false },
      },
    ],
    dataZoom: [
      {
        type: 'inside',
        xAxisIndex: [0, 1],
        start: 60,
        end: 100,
      },
      {
        show: true,
        type: 'slider',
        xAxisIndex: [0, 1],
        top: '94%',
        start: 60,
        end: 100,
        height: 20,
        borderColor: '#2a2a42',
        fillerColor: 'rgba(232, 128, 78, 0.1)',
        handleStyle: { color: '#e8804e' },
        textStyle: { color: '#9090a8' },
      },
    ],
    series: [
      {
        name: 'K线',
        type: 'candlestick',
        data: ohlc,
        itemStyle: {
          color: '#ef4444',
          color0: '#22c55e',
          borderColor: '#ef4444',
          borderColor0: '#22c55e',
        },
        markPoint: allMarkers.length > 0 ? {
          data: allMarkers,
          animation: false,
        } : undefined,
      },
      {
        name: '成交量',
        type: 'bar',
        xAxisIndex: 1,
        yAxisIndex: 1,
        data: volumes.map((v, i) => ({
          value: v,
          itemStyle: {
            color: klines[i].close >= klines[i].open ? '#ef4444' : '#22c55e',
            opacity: 0.7,
          },
        })),
      },
    ],
  };

  return (
    <ReactECharts
      option={option}
      style={{ height: `${height}px`, width: '100%' }}
      opts={{ renderer: 'canvas' }}
    />
  );
}
