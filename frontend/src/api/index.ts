const API_BASE = '/api/v1';

export interface Quote {
  code: string;
  name: string;
  price: number;
  pct_change: number;
  change: number;
  open: number;
  pre_close: number;
  high: number;
  low: number;
  volume: number;
  amount: number;
  timestamp: number;
}

export interface Kline {
  stock_code: string;
  date: string;
  open: number;
  close: number;
  high: number;
  low: number;
  volume: number;
  amount: number;
  pct_change: number | null;
  turnover: number | null;
}

export interface IndexSummary {
  code: string;
  name: string;
  price: number;
  pct_change: number;
  change: number;
  volume: number;
  amount: number;
}

export interface WatchlistItem {
  stock_code: string;
  cost_price: number;
  shares: number;
}

export interface MarketOverview {
  indices: IndexSummary[];
  up_count: number;
  down_count: number;
  flat_count: number;
  limit_up_count: number;
  limit_down_count: number;
  total_amount: number;
  timestamp: number;
}

export interface SectorRank {
  code: string;
  name: string;
  sector_type: string;
  change_pct: number;
  /// 领涨股名称（仅展示）
  leader_stock: string | null;
  /// 领涨股代码（查询用）
  leader_code: string | null;
  leader_pct: number | null;
  net_amount: number | null;
  up_count: number | null;
  down_count: number | null;
}

export interface SectorStock {
  code: string;
  name: string;
  price: number;
  pct_change: number;
  change: number;
  volume: number;
  amount: number;
  high: number;
  low: number;
  open: number;
  pre_close: number;
  turnover: number;
}

export interface SectorStocksResponse {
  stocks: SectorStock[];
  total: number;
  page: number;
  size: number;
}

export interface TradeSignal {
  signal_type: 'Buy' | 'Sell';
  date: string;
  price: number;
  source: string;
  strength: string;
  description: string;
}

export interface PriceLevel {
  price: number;
  strength: 'Strong' | 'Medium' | 'Weak';
  source: string;
}

export interface SupportResistance {
  supports: PriceLevel[];
  resistances: PriceLevel[];
  current_price: number;
}

export interface SignalAnalysis {
  signals: TradeSignal[];
  current_signals: TradeSignal[];
  macd: { date: string; dif: number; dea: number; macd: number }[];
  kdj: { date: string; k: number; d: number; j: number }[];
  rsi: { date: string; rsi6: number; rsi12: number; rsi24: number }[];
  boll: { date: string; upper: number; mid: number; lower: number }[];
  support_resistance: SupportResistance;
  overall_score: number;
  overall_action: string;
}

// ============ 量价分析 ============

export type VpRelationship =
  | 'VolumeUpPriceUp'
  | 'VolumeUpPriceDown'
  | 'VolumeFlatPriceUp'
  | 'VolumeFlatPriceDown'
  | 'VolumeDownPriceUp'
  | 'VolumeDownPriceDown'
  | 'Neutral';

export type VpSignalType = 'Buy' | 'Sell' | 'Watch';

export interface VpBar {
  date: string;
  close: number;
  volume: number;
  amount: number;
  pct_change: number;
  volume_ratio: number;
  relationship: VpRelationship;
  note: string;
}

export interface VpSignal {
  signal_type: VpSignalType;
  name: string;
  date: string;
  strength: string;
  description: string;
}

export interface VolumePriceAnalysis {
  summary: string;
  verdict: string;
  volume_status: string;
  current_volume_ratio: number;
  recent_bars: VpBar[];
  signals: VpSignal[];
  conclusion: string;
}

// ============ 盘口解读（Tape Reading） ============

export interface FlowPoint {
  date: string;
  main_net_inflow: number;
  main_net_ratio: number;
  pct_change: number;
}

export interface DragonTigerSummary {
  date: string;
  reason: string;
  net_amount: number;
  institution_net_buy: number;
  hot_money_tags: string[];
}

export interface TapeReadingSignal {
  category: string;
  name: string;
  strength: number;
  detail: string;
}

export interface TapeReadingAnalysis {
  stock_code: string;
  verdict: string;
  confidence: string;
  main_participation: number;
  big_order_net: number;
  wash_suspect: boolean;
  retail_driven: boolean;
  recent_flow: FlowPoint[];
  dragon_tiger: DragonTigerSummary | null;
  signals: TapeReadingSignal[];
  conclusion: string;
}


// ============ Phase 4: 资金流向 ============

export interface CapitalFlow {
  stock_code: string;
  date: string;
  main_net_inflow: number;
  main_net_ratio: number;
  huge_net_inflow: number;
  huge_net_ratio: number;
  large_net_inflow: number;
  large_net_ratio: number;
  medium_net_inflow: number;
  medium_net_ratio: number;
  small_net_inflow: number;
  small_net_ratio: number;
  close_price: number;
  pct_change: number;
}

export interface SectorCapitalFlow {
  code: string;
  name: string;
  main_net_inflow: number;
  main_net_ratio: number;
  huge_net_inflow: number;
  large_net_inflow: number;
  medium_net_inflow: number;
  small_net_inflow: number;
  pct_change: number;
  stock_count: number | null;
}

// ============ 市场热点板块 ============

export interface ReasonFactor {
  category: string;
  name: string;
  weight: number;
  detail: string;
}

export interface SectorUpReason {
  logic: string;
  technical: string;
  fundamental: string;
  catalyst: string;
  factors: ReasonFactor[];
}

export interface SectorLeaderFundamental {
  code: string;
  name: string;
  pct_change: number | null;
  revenue_yoy: number | null;
  net_profit_yoy: number | null;
  roe: number | null;
  gross_margin: number | null;
  net_margin: number | null;
  report_date: string;
}

export interface SectorHotspotItem {
  rank: number;
  code: string;
  name: string;
  sector_type: string;
  change_pct: number;
  /// 领涨股名称（仅展示）
  leader_stock: string | null;
  /// 领涨股代码（查询用）
  leader_code: string | null;
  leader_pct: number | null;
  up_count: number;
  down_count: number;
  main_net_inflow: number;
  main_net_ratio: number;
  strength_score: number;
  leader_fundamental: SectorLeaderFundamental | null;
  dragon_tiger_hit: boolean;
  reason: SectorUpReason;
  summary: string;
}

export interface SectorHotspotAnalysis {
  generated_at: number;
  industry_count: number;
  concept_count: number;
  hot_sectors: SectorHotspotItem[];
  conclusion: string;
}

// ============ 做T策略（KDJ + 布林带六步法则） ============

/** 做T方向：正T=低吸高抛，反T=高抛低吸，双模=自动 */
export type TMode = 'positive' | 'reverse' | 'both';

/** 出场模式：中轨止盈（推荐）/ 上轨止盈（原始六步）/ 先到先得 */
export type ExitMode = 'mid_band' | 'upper_band' | 'either';

export type TDirection = 'buy' | 'sell';

/** 信号动作：建仓 / 加仓 / 减仓 / 清仓 */
export type SignalAction = 'entry' | 'add' | 'reduce' | 'clear';

/** 一笔做T中的一次成交（分批时，一笔交易会包含多个 leg） */
export interface TradeLeg {
  date: string;
  action: string;
  price: number;
  shares: number;
  amount: number;
  reason: string;
}

export interface TStrategyConfig {
  kdj_n: number;
  j_oversold: number;
  j_overbought: number;
  /** 警戒激活后的有效窗口（根）。J 超卖与金叉几乎不同日，必须允许等待 */
  signal_window: number;
  boll_n: number;
  boll_k: number;
  touch_tol: number;
  /** 只在布林平行震荡时做T：信号稀少但质量高 */
  trend_filter: boolean;
  /** 缩量确认：盈亏比显著提升 */
  volume_filter: boolean;
  volume_ratio: number;
  use_divergence: boolean;
  exit_mode: ExitMode;
  stop_loss: number;
  max_hold: number;
  /** 分批建仓：建仓后价格继续朝有利方向运行时分批补仓 */
  split_entry: boolean;
  /** 最多分几批建仓 */
  max_entries: number;
  /** 每批之间的价格间隔 */
  add_step: number;
  /** 分批止盈：到中轨先平一部分，剩余等上轨 */
  split_exit: boolean;
  /** 首批止盈比例 */
  first_exit_ratio: number;
  /** 按信号强度动态调仓：强信号重仓、弱信号轻仓 */
  dynamic_position: boolean;
  /** 动态仓位下限（弱信号） */
  min_position_ratio: number;
  /** 动态仓位上限（强信号） */
  max_position_ratio: number;
  t_mode: TMode;
  position_ratio: number;
  cost_rate: number;
  base_capital: number;
}

export interface TSignal {
  date: string;
  index: number;
  direction: TDirection;
  action: SignalAction;
  /** 本次操作占底仓的比例 */
  position_pct: number;
  price: number;
  step: string;
  t_type: string;
  strength: number;
  reasons: string[];
  boll_shape: string;
  j_value: number;
  has_divergence: boolean;
  volume_shrunk: boolean;
}

export interface TTrade {
  seq: number;
  t_type: string;
  entry_date: string;
  entry_price: number;
  exit_date: string;
  exit_price: number;
  shares: number;
  gross_pnl_pct: number;
  net_pnl_pct: number;
  pnl_amount: number;
  hold_bars: number;
  exit_reason: string;
  entry_strength: number;
  /** 实际使用的仓位比例（动态仓位时会偏离 position_ratio） */
  used_position_ratio: number;
  /** 分批成交明细（不分批时长度为 2：一买一卖） */
  legs: TradeLeg[];
}

export interface TBacktestStats {
  total_trades: number;
  win_trades: number;
  loss_trades: number;
  win_rate: number;
  avg_pnl_pct: number;
  total_pnl_pct: number;
  profit_factor: number;
  avg_hold_bars: number;
  max_win_pct: number;
  max_loss_pct: number;
  max_drawdown_pct: number;
  signal_count: number;
  positive_trades: number;
  reverse_trades: number;
  stop_loss_exits: number;
  timeout_exits: number;
}

export interface EquityPoint {
  date: string;
  cumulative_pct: number;
}

export interface TBacktestResult {
  code: string;
  period: string;
  start_date: string;
  end_date: string;
  bar_count: number;
  config: TStrategyConfig;
  trades: TTrade[];
  stats: TBacktestStats;
  equity_curve: EquityPoint[];
  signals: TSignal[];
  conclusion: string;
}

export interface TScanItem {
  label: string;
  config: TStrategyConfig;
  stats: TBacktestStats;
  total_pnl_pct: number;
}

export interface TBacktestRequest {
  period?: string;
  limit?: number;
  config?: Partial<TStrategyConfig>;
}

export const DEFAULT_T_CONFIG: TStrategyConfig = {
  kdj_n: 9,
  j_oversold: 10,
  j_overbought: 80,
  signal_window: 8,
  boll_n: 20,
  boll_k: 2,
  touch_tol: 0.02,
  trend_filter: false,
  volume_filter: true,
  volume_ratio: 0.85,
  use_divergence: true,
  exit_mode: 'mid_band',
  stop_loss: 0.05,
  max_hold: 20,
  split_entry: false,
  max_entries: 3,
  add_step: 0.02,
  split_exit: false,
  first_exit_ratio: 0.5,
  dynamic_position: false,
  min_position_ratio: 0.15,
  max_position_ratio: 0.5,
  t_mode: 'both',
  position_ratio: 1 / 3,
  cost_rate: 0.0012,
  base_capital: 1_000_000,
};

export interface NorthBoundFlow {
  date: string;
  sh_net_inflow: number;
  sz_net_inflow: number;
  total_net_inflow: number;
  sh_balance: number;
  sz_balance: number;
}

export interface NorthBoundHoldings {
  date: string;
  hold_market_cap: number;
  add_market_cap: number;
  change_rate: number;
}

export interface MarketCapitalFlow {
  date: string;
  main_net_inflow: number;
  huge_net_inflow: number;
  large_net_inflow: number;
  medium_net_inflow: number;
  small_net_inflow: number;}

export interface MarginTrading {
  date: string;
  margin_balance: number;
  short_balance: number;
  total_balance: number;
  margin_buy: number;
  margin_repay: number;
  margin_net_buy: number;
  short_sell_vol: number;
  short_repay_vol: number;
  short_net_sell: number;
}

export interface CapitalFlowOverview {
  north_bound: NorthBoundFlow | null;
  north_bound_holdings: NorthBoundHoldings | null;
  margin_trading: MarginTrading | null;
  market_capital_flow: MarketCapitalFlow | null;
  sector_flows: SectorCapitalFlow[];
  timestamp: number;
}

// ============ Phase 4: 涨停复盘 ============

export interface LimitUpStock {
  code: string;
  name: string;
  price: number;
  pct_change: number;
  consecutive_days: number;
  first_limit_time: string | null;
  last_limit_time: string | null;
  break_count: number;
  seal_volume: number;
  seal_amount: number;
  seal_strength: number | null;
  amount: number;
  turnover: number;
  float_market_cap: number;
  industry: string | null;
  reason: string | null;
  amplitude: number;
}

export interface LimitDownStock {
  code: string;
  name: string;
  price: number;
  pct_change: number;
  consecutive_days: number;
  seal_amount: number;
  amount: number;
  turnover: number;
  industry: string | null;
}

export interface BrokenBoardStock {
  code: string;
  name: string;
  price: number;
  pct_change: number;
  max_pct: number;
  break_count: number;
  amount: number;
  turnover: number;
  industry: string | null;
}

export interface ConsecutiveBoardTier {
  days: number;
  count: number;
  stocks: LimitUpStock[];
}

export interface LimitUpReview {
  limit_ups: LimitUpStock[];
  limit_up_count: number;
  one_word_count: number;
  t_shape_count: number;
  broken_boards: BrokenBoardStock[];
  broken_count: number;
  limit_downs: LimitDownStock[];
  limit_down_count: number;
  consecutive_tiers: ConsecutiveBoardTier[];
  max_consecutive: number;
  reason_distribution: [string, number][];
  break_rate: number;
  timestamp: number;
}

// ============ Phase 4: 龙虎榜 ============

export interface BrokerSeat {
  name: string;
  buy_amount: number;
  sell_amount: number;
  net_amount: number;
}

export interface DragonTigerEntry {
  code: string;
  name: string;
  date: string;
  close_price: number;
  pct_change: number;
  turnover: number;
  amount: number;
  amplitude: number;
  net_amount: number;
  total_buy: number;
  total_sell: number;
  lhb_amount_ratio: number;
  reason: string;
  buy_seats: BrokerSeat[];
  sell_seats: BrokerSeat[];
  institution_net_buy: number;
  hot_money_tags: string[];
}

export interface DragonTigerOverview {
  total_count: number;
  net_buy_count: number;
  net_sell_count: number;
  institution_net_buy_total: number;
  entries: DragonTigerEntry[];
  active_seats: BrokerSeat[];
  timestamp: number;
}

// ============ Phase 4: 财务数据 ============

export interface FinancialIndicator {
  /// 报告期 (YYYY-MM-DD)
  report_date: string;
  /// 报告期类型：年报 / 中报 / 一季报 / 三季报
  report_type: string | null;
  /// 以下金额类字段单位为**万元**；每股类为元；比率类为百分数
  eps: number | null;
  bvps: number | null;
  cfps: number | null;
  roe: number | null;
  roa: number | null;
  gross_margin: number | null;
  net_margin: number | null;
  revenue: number | null;
  revenue_yoy: number | null;
  net_profit: number | null;
  net_profit_yoy: number | null;
  deducted_profit: number | null;
  deducted_profit_yoy: number | null;
  debt_ratio: number | null;
  current_ratio: number | null;
  quick_ratio: number | null;
}

export interface FinancialSummary {
  stock_code: string;
  stock_name: string;
  latest_report_date: string;
  pe: number | null;
  pb: number | null;
  ps: number | null;
  total_market_cap: number | null;
  float_market_cap: number | null;
  indicators: FinancialIndicator[];
}

// ============ AI 分析历史 ============

export type AnalysisType = 'diagnose' | 'predict' | 'market' | 'chat';

export interface AiHistoryItem {
  id: number;
  analysis_type: AnalysisType;
  title: string;
  stock_code: string | null;
  stock_name: string | null;
  question: string | null;
  summary: string;
  model: string | null;
  created_at: string;
}

export interface AiAnalysisHistory {
  id: number;
  user_id: string;
  analysis_type: AnalysisType;
  title: string;
  stock_code: string | null;
  stock_name: string | null;
  question: string | null;
  content: string;
  model: string | null;
  created_at: string;
}

async function request<T>(url: string): Promise<T> {
  const resp = await fetch(url);
  if (!resp.ok) {
    const errText = await resp.text();
    try {
      const errJson = JSON.parse(errText);
      throw new Error(errJson.error || errJson.message || `请求失败 (${resp.status})`);
    } catch (e) {
      if (e instanceof Error && e.message !== errText) throw e;
      throw new Error(errText || `请求失败 (${resp.status})`);
    }
  }
  return resp.json();
}

/// 解析错误响应（支持 JSON 和纯文本）
async function parseErrorResponse(resp: Response): Promise<string> {
  const text = await resp.text();
  try {
    const json = JSON.parse(text);
    if (json.error) return json.error;
    if (json.message) return json.message;
    return text || `请求失败 (${resp.status})`;
  } catch {
    return text || `请求失败 (${resp.status})`;
  }
}

/// POST JSON 请求（用于参数较多的接口，如做T回测配置）
async function postJson<T>(url: string, body: unknown): Promise<T> {
  const resp = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!resp.ok) {
    throw new Error(await parseErrorResponse(resp));
  }
  return resp.json();
}

/// SSE 流式读取
async function streamSSE(
  url: string,
  onChunk: (text: string) => void,
  onDone?: () => void,
  onError?: (err: string) => void,
): Promise<void> {
  try {
    const resp = await fetch(url);
    if (!resp.ok) {
      onError?.(await parseErrorResponse(resp));
      return;
    }
    // 检查 Content-Type 是否为 SSE
    const contentType = resp.headers.get('content-type') || '';
    if (!contentType.includes('text/event-stream')) {
      onError?.(await parseErrorResponse(resp));
      return;
    }
    const reader = resp.body?.getReader();
    if (!reader) {
      onError?.('无法读取响应流');
      return;
    }
    const decoder = new TextDecoder();
    let buffer = '';
    const dataLines: string[] = [];

    const flushEvent = () => {
      if (dataLines.length > 0) {
        const data = dataLines.join('\n');
        if (data !== '[DONE]') {
          onChunk(data);
        }
        dataLines.length = 0;
      }
    };

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split('\n');
      buffer = lines.pop() || '';
      for (const line of lines) {
        const trimmed = line.trim();
        if (trimmed === '') {
          // 空行表示事件结束
          flushEvent();
        } else if (trimmed.startsWith('data:')) {
          const data = trimmed.slice(5).trim();
          dataLines.push(data);
        }
      }
    }
    // 流结束：刷新剩余数据
    flushEvent();
    onDone?.();
  } catch (e) {
    onError?.(e instanceof Error ? e.message : '未知错误');
  }
}

/// POST SSE 流式读取（用于 chat）
async function streamPostSSE(
  url: string,
  body: unknown,
  onChunk: (text: string) => void,
  onDone?: () => void,
  onError?: (err: string) => void,
): Promise<void> {
  try {
    const resp = await fetch(url, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(body),
    });
    if (!resp.ok) {
      onError?.(await parseErrorResponse(resp));
      return;
    }
    // 检查 Content-Type 是否为 SSE
    const contentType = resp.headers.get('content-type') || '';
    if (!contentType.includes('text/event-stream')) {
      onError?.(await parseErrorResponse(resp));
      return;
    }
    const reader = resp.body?.getReader();
    if (!reader) {
      onError?.('无法读取响应流');
      return;
    }
    const decoder = new TextDecoder();
    let buffer = '';
    const dataLines: string[] = [];

    const flushEvent = () => {
      if (dataLines.length > 0) {
        const data = dataLines.join('\n');
        if (data !== '[DONE]') {
          onChunk(data);
        }
        dataLines.length = 0;
      }
    };

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split('\n');
      buffer = lines.pop() || '';
      for (const line of lines) {
        const trimmed = line.trim();
        if (trimmed === '') {
          // 空行表示事件结束
          flushEvent();
        } else if (trimmed.startsWith('data:')) {
          const data = trimmed.slice(5).trim();
          dataLines.push(data);
        }
      }
    }
    // 流结束：刷新剩余数据
    flushEvent();
    onDone?.();
  } catch (e) {
    onError?.(e instanceof Error ? e.message : '未知错误');
  }
}

// ============ 市场情绪日报 ============

/** 题材个股（同花顺强势股条目） */
export interface ThemeStock {
  code: string;
  name: string;
  /** 涨跌幅 (%) */
  pct_change: number;
  /** 原始归因，多个题材以「+」连接 */
  reason: string;
}

export interface HotTheme {
  tag: string;
  count: number;
  leader: string | null;
  leader_pct: number;
  avg_pct: number;
  stocks: ThemeStock[];
}

export interface ThemeSummary {
  stock_count: number;
  tag_total: number;
  theme_total: number;
  themes: HotTheme[];
  top_tag: string | null;
}

export interface ReasonStat {
  reason: string;
  count: number;
}

/**
 * 龙虎榜汇总
 *
 * 口径注意：`net_total` 为按个股去重后的合计，`records_sum` 为原始记录直接求和。
 * 同一只股票触发多个上榜原因时东财会返回多条记录且净买入额可能不同。
 */
export interface LhbSummary {
  stock_count: number;
  record_count: number;
  net_total: number;
  records_sum: number;
  buy_total: number;
  sell_total: number;
  net_buy_count: number;
  net_sell_count: number;
  amount_total: number;
  entries: DragonTigerEntry[];
  top_reasons: ReasonStat[];
}

export interface IndustryBoard {
  name: string;
  change_pct: number;
  up_count: number;
  down_count: number;
  flat_count: number;
  leader: string | null;
  leader_code: string | null;
  leader_pct: number;
  /** 历史交易日为 null：可用日线源不含成交额，不做估算 */
  amount: number | null;
  member_count: number;
  calc_count: number;
}

export interface IndexDailyQuote {
  code: string;
  name: string;
  close: number;
  pct_change: number;
}

export interface MarketBreadth {
  up_count: number;
  down_count: number;
  flat_count: number;
  covered_count: number;
  total_amount: number | null;
  indices: IndexDailyQuote[];
}

export interface Methodology {
  board_method: string;
  from_cache: boolean;
  elapsed_ms: number;
  notes: string[];
}

export interface SentimentDaily {
  date: string;
  generated_at: number;
  lhb: LhbSummary;
  themes: ThemeSummary;
  boards: IndustryBoard[];
  breadth: MarketBreadth;
  methodology: Methodology;
  sources: string[];
}

/** 归档目录里的一天，只带「要不要点开」所需的最小信息 */
export interface SentimentArchiveEntry {
  date: string;
  generated_at: number;
  payload_bytes: number;
  covered_count: number;
  board_count: number;
  /** 服务端是否留存了导出件，有则「下载 HTML」可直接取、不再重新渲染 */
  has_export: boolean;
}

/** 定时任务的一次运行记录。outcome: succeeded | skipped | failed */
export interface JobRunInfo {
  job: string;
  date: string;
  outcome: string;
  detail: string | null;
  elapsed_ms: number | null;
  started_at: number;
}

export interface SentimentArchiveList {
  total: number;
  entries: SentimentArchiveEntry[];
  /** 自动归档最近一次运行记录；未开启自动归档时为 null */
  last_run: JobRunInfo | null;
}

export const api = {
  // 行情接口
  getQuote: (code: string) =>
    request<Quote>(`${API_BASE}/quotes/${code}`),

  getQuotes: (codes: string[]) =>
    request<Quote[]>(`${API_BASE}/quotes?codes=${codes.join(',')}`),

  getKline: (code: string, period = 'daily', limit = 365) =>
    request<Kline[]>(`${API_BASE}/kline/${code}?period=${period}&limit=${limit}`),

  getMarketOverview: () =>
    request<MarketOverview>(`${API_BASE}/market/overview`),

  getSectorRanks: (type: string = 'industry') =>
    request<SectorRank[]>(`${API_BASE}/market/sectors?type=${type}`),

  getSectorStocks: (code: string, page: number = 1, size: number = 50) =>
    request<SectorStocksResponse>(`${API_BASE}/market/sectors/${code}/stocks?page=${page}&size=${size}`),

  getTradeSignals: (code: string, period: string = 'daily', limit: number = 120) =>
    request<SignalAnalysis>(`${API_BASE}/analysis/signals/${code}?period=${period}&limit=${limit}`),

  // 自选股
  getWatchlist: () =>
    request<WatchlistItem[]>(`${API_BASE}/watchlist`),

  addToWatchlist: (code: string, cost_price?: number, shares?: number) =>
    fetch(`${API_BASE}/watchlist`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ code, cost_price: cost_price ?? 0, shares: shares ?? 0 }),
    }),

  updateWatchlist: (code: string, cost_price: number, shares: number) =>
    fetch(`${API_BASE}/watchlist/${code}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ cost_price, shares }),
    }),

  removeFromWatchlist: (code: string) =>
    fetch(`${API_BASE}/watchlist/${code}`, { method: 'DELETE' }),

  // AI 研判接口
  getAiStatus: () =>
    request<{ configured: boolean; model: string }>(`${API_BASE}/ai/status`),

  diagnoseStock: (code: string) =>
    request<{ content: string }>(`${API_BASE}/ai/diagnose/${code}`),

  diagnoseStockStream: (
    code: string,
    onChunk: (text: string) => void,
    onDone?: () => void,
    onError?: (err: string) => void,
  ) => streamSSE(`${API_BASE}/ai/diagnose/${code}/stream`, onChunk, onDone, onError),

  marketAnalysis: () =>
    request<{ content: string }>(`${API_BASE}/ai/market`),

  marketAnalysisStream: (
    onChunk: (text: string) => void,
    onDone?: () => void,
    onError?: (err: string) => void,
  ) => streamSSE(`${API_BASE}/ai/market/stream`, onChunk, onDone, onError),

  predictTrend: (code: string) =>
    request<{ content: string }>(`${API_BASE}/ai/predict/${code}`),

  predictTrendStream: (
    code: string,
    onChunk: (text: string) => void,
    onDone?: () => void,
    onError?: (err: string) => void,
  ) => streamSSE(`${API_BASE}/ai/predict/${code}/stream`, onChunk, onDone, onError),

  chat: (question: string) =>
    request<{ content: string }>(`${API_BASE}/ai/chat`, ),

  chatStream: (
    question: string,
    onChunk: (text: string) => void,
    onDone?: () => void,
    onError?: (err: string) => void,
  ) => streamPostSSE(`${API_BASE}/ai/chat/stream`, { question }, onChunk, onDone, onError),

  // ============ Phase 4: 资金流向 ============
  getCapitalFlow: (code: string, limit = 60) =>
    request<CapitalFlow[]>(`${API_BASE}/capital/${code}?limit=${limit}`),

  getSectorCapitalFlows: () =>
    request<SectorCapitalFlow[]>(`${API_BASE}/capital/sectors`),

  getNorthBoundFlow: (limit = 30) =>
    request<NorthBoundFlow[]>(`${API_BASE}/capital/north-bound?limit=${limit}`),

  getNorthBoundHoldings: () =>
    request<NorthBoundHoldings[]>(`${API_BASE}/capital/north-holdings`),

  getMarketCapitalFlow: () =>
    request<MarketCapitalFlow>(`${API_BASE}/capital/market-flow`),

  getMarginTrading: (limit = 30) =>
    request<MarginTrading[]>(`${API_BASE}/capital/margin?limit=${limit}`),

  getCapitalOverview: () =>
    request<CapitalFlowOverview>(`${API_BASE}/capital/overview`),

  // ============ Phase 4: 涨停复盘 ============
  getLimitUpReview: () =>
    request<LimitUpReview>(`${API_BASE}/limitup/review`),

  getLimitUps: () =>
    request<LimitUpStock[]>(`${API_BASE}/limitup/ups`),

  getLimitDowns: () =>
    request<LimitDownStock[]>(`${API_BASE}/limitup/downs`),

  getBrokenBoards: () =>
    request<BrokenBoardStock[]>(`${API_BASE}/limitup/broken`),

  // ============ Phase 4: 龙虎榜 ============
  getDragonTiger: (date?: string) =>
    request<DragonTigerEntry[]>(`${API_BASE}/dragontiger/list${date ? `?date=${date}` : ''}`),

  getDragonTigerOverview: (date?: string) =>
    request<DragonTigerOverview>(`${API_BASE}/dragontiger/overview${date ? `?date=${date}` : ''}`),

  // ============ Phase 4: 财务数据 ============
  getFinancialIndicators: (code: string) =>
    request<FinancialSummary>(`${API_BASE}/finance/${code}`),

  // ============ AI 分析历史 ============
  getAiHistory: (type?: AnalysisType, limit = 50, offset = 0) => {
    const params = new URLSearchParams();
    if (type) params.set('type', type);
    params.set('limit', String(limit));
    params.set('offset', String(offset));
    return request<AiHistoryItem[]>(`${API_BASE}/ai/history?${params.toString()}`);
  },

  getAiHistoryDetail: (id: number) =>
    request<AiAnalysisHistory>(`${API_BASE}/ai/history/${id}`),

  deleteAiHistory: async (id: number) => {
    const resp = await fetch(`${API_BASE}/ai/history/${id}`, { method: 'DELETE' });
    if (!resp.ok && resp.status !== 204) {
      throw new Error('删除失败');
    }
  },

  // ============ 量价分析 ============
  getVolumePriceAnalysis: (code: string, period: string = 'daily', limit: number = 120) =>
    request<VolumePriceAnalysis>(
      `${API_BASE}/analysis/volume-price/${code}?period=${period}&limit=${limit}`,
    ),

  // ============ 盘口解读 ============
  getTapeReading: (code: string, period: string = 'daily', limit: number = 120) =>
    request<TapeReadingAnalysis>(
      `${API_BASE}/analysis/tape-reading/${code}?period=${period}&limit=${limit}`,
    ),

  // ============ 市场热点板块 ============
  getSectorHotspot: (top: number = 12, includeFundamental: boolean = true) =>
    request<SectorHotspotAnalysis>(
      `${API_BASE}/sector/hotspot?top=${top}&include_fundamental=${includeFundamental}`,
    ),

  // ============ 做T策略回测 ============
  runTBacktest: (code: string, req: TBacktestRequest) =>
    postJson<TBacktestResult>(`${API_BASE}/t/backtest/${code}`, req),

  scanTParams: (code: string, period: string = 'daily', limit: number = 500) =>
    postJson<TScanItem[]>(`${API_BASE}/t/scan/${code}`, { period, limit }),

  // ============ 市场情绪日报 ============
  getSentimentDaily: (date?: string, refresh = false) => {
    const params = new URLSearchParams();
    if (date) params.set('date', date);
    if (refresh) params.set('refresh', 'true');
    const qs = params.toString();
    return request<SentimentDaily>(
      `${API_BASE}/sentiment/daily${qs ? `?${qs}` : ''}`,
    );
  },

  /**
   * 单文件 HTML 日报的下载地址。
   * 后端以 Content-Disposition: attachment 返回，直接跳转即可触发下载。
   */
  sentimentExportUrl: (date?: string): string => {
    const params = new URLSearchParams();
    if (date) params.set('date', date);
    const qs = params.toString();
    return `${API_BASE}/sentiment/daily/export${qs ? `?${qs}` : ''}`;
  },

  /** 已归档日报目录，按日期倒序。用于告诉用户哪些天能秒开。 */
  listSentimentDates: (limit?: number) => {
    const params = new URLSearchParams();
    if (limit) params.set('limit', String(limit));
    const qs = params.toString();
    return request<SentimentArchiveList>(
      `${API_BASE}/sentiment/dates${qs ? `?${qs}` : ''}`,
    );
  },
};
