import { create } from 'zustand';
import { api, Quote, MarketOverview, SectorRank, WatchlistItem } from '../api';

interface StockState {
  // 搜索
  searchCode: string;
  setSearchCode: (code: string) => void;

  // 实时行情
  quotes: Quote[];
  loadingQuotes: boolean;
  fetchQuotes: (codes: string[]) => Promise<void>;

  // 大盘总览
  marketOverview: MarketOverview | null;
  loadingMarket: boolean;
  fetchMarketOverview: () => Promise<void>;

  // 板块排行
  sectorRanks: SectorRank[];
  loadingSectors: boolean;
  fetchSectorRanks: (type: string) => Promise<void>;

  // 自选股
  watchlist: WatchlistItem[];
  fetchWatchlist: () => Promise<void>;
  addStock: (code: string) => Promise<void>;
  removeStock: (code: string) => Promise<void>;

  // 自选股行情
  watchQuotes: Quote[];
  fetchWatchQuotes: () => Promise<void>;
}

export const useStockStore = create<StockState>((set, get) => ({
  searchCode: '000001',
  setSearchCode: (code) => set({ searchCode: code }),

  quotes: [],
  loadingQuotes: false,
  fetchQuotes: async (codes) => {
    if (codes.length === 0) return;
    set({ loadingQuotes: true });
    try {
      const quotes = await api.getQuotes(codes);
      set({ quotes, loadingQuotes: false });
    } catch (e) {
      console.error('Failed to fetch quotes:', e);
      set({ loadingQuotes: false });
    }
  },

  marketOverview: null,
  loadingMarket: false,
  fetchMarketOverview: async () => {
    set({ loadingMarket: true });
    try {
      const overview = await api.getMarketOverview();
      set({ marketOverview: overview, loadingMarket: false });
    } catch (e) {
      console.error('Failed to fetch market overview:', e);
      set({ loadingMarket: false });
    }
  },

  sectorRanks: [],
  loadingSectors: false,
  fetchSectorRanks: async (type) => {
    set({ loadingSectors: true });
    try {
      const ranks = await api.getSectorRanks(type);
      set({ sectorRanks: ranks, loadingSectors: false });
    } catch (e) {
      console.error('Failed to fetch sector ranks:', e);
      set({ loadingSectors: false });
    }
  },

  watchlist: [],
  fetchWatchlist: async () => {
    try {
      const list = await api.getWatchlist();
      set({ watchlist: list });
    } catch (e) {
      console.error('Failed to fetch watchlist:', e);
    }
  },
  addStock: async (code) => {
    await api.addToWatchlist(code);
    await get().fetchWatchlist();
  },
  removeStock: async (code) => {
    await api.removeFromWatchlist(code);
    await get().fetchWatchlist();
  },

  watchQuotes: [],
  fetchWatchQuotes: async () => {
    const { watchlist } = get();
    if (watchlist.length === 0) {
      set({ watchQuotes: [] });
      return;
    }
    try {
      const quotes = await api.getQuotes(watchlist.map((w) => w.stock_code));
      set({ watchQuotes: quotes });
    } catch (e) {
      console.error('Failed to fetch watchlist quotes:', e);
    }
  },
}));
