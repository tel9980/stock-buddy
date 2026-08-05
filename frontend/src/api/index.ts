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
  leader_stock: string | null;
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
  small_net_inflow: number;
}

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
  report_date: string;
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
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split('\n');
      buffer = lines.pop() || '';
      for (const line of lines) {
        const trimmed = line.trim();
        if (trimmed.startsWith('data:')) {
          const data = trimmed.slice(5).trim();
          if (data === '[DONE]') continue;
          if (data) onChunk(data);
        }
      }
    }
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
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split('\n');
      buffer = lines.pop() || '';
      for (const line of lines) {
        const trimmed = line.trim();
        if (trimmed.startsWith('data:')) {
          const data = trimmed.slice(5).trim();
          if (data === '[DONE]') continue;
          if (data) onChunk(data);
        }
      }
    }
    onDone?.();
  } catch (e) {
    onError?.(e instanceof Error ? e.message : '未知错误');
  }
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
    request<string[]>(`${API_BASE}/watchlist`),

  addToWatchlist: (code: string) =>
    fetch(`${API_BASE}/watchlist`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ code }),
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
};
