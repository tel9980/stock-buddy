import { create } from 'zustand';
import { persist } from 'zustand/middleware';
import { DEFAULT_T_CONFIG } from '../api';
import type { TBacktestResult, TScanItem, TStrategyConfig } from '../api';

/**
 * 做T回测页的状态。
 *
 * 为什么不放组件内部 useState：路由切换会卸载组件，useState 全部丢失，
 * 切回页面时用户填的参数和跑出的结果都会没。这里提到全局 store，
 * 并用 persist 持久化到 localStorage，连刷新浏览器也保留。
 */
interface TBacktestState {
  // ── 表单 ──
  code: string;
  period: string;
  limit: number;
  cfg: TStrategyConfig;
  showAdvanced: boolean;
  activeTab: string;

  // ── 结果 ──
  result: TBacktestResult | null;
  scan: TScanItem[] | null;

  setCode: (v: string) => void;
  setPeriod: (v: string) => void;
  setLimit: (v: number) => void;
  patchConfig: (p: Partial<TStrategyConfig>) => void;
  resetConfig: () => void;
  setShowAdvanced: (v: boolean) => void;
  setActiveTab: (v: string) => void;
  setResult: (r: TBacktestResult | null) => void;
  setScan: (s: TScanItem[] | null) => void;
}

/**
 * 校验持久化的回测结果是否结构完整。
 *
 * 后台新增字段后，浏览器里已存的老结果会缺字段；与其让页面渲染时崩溃，
 * 不如在读取阶段就识别出来并丢弃。
 */
function isValidResult(r: unknown): r is TBacktestResult {
  if (!r || typeof r !== 'object') return false;
  const t = r as TBacktestResult;
  if (!Array.isArray(t.trades) || !t.stats || !t.config) return false;
  // v2 起每笔交易必须带 legs
  return t.trades.every((x) => Array.isArray(x.legs) && x.used_position_ratio !== undefined);
}

function isValidScan(s: unknown): s is TScanItem[] {
  if (!Array.isArray(s)) return false;
  return s.every((x) => x && x.stats && x.config !== undefined);
}

export const useTBacktestStore = create<TBacktestState>()(
  persist(
    (set) => ({
      code: '000001',
      period: 'daily',
      limit: 500,
      cfg: DEFAULT_T_CONFIG,
      showAdvanced: false,
      activeTab: 'single',
      result: null,
      scan: null,

      setCode: (code) => set({ code }),
      setPeriod: (period) => set({ period }),
      setLimit: (limit) => set({ limit }),
      patchConfig: (p) => set((s) => ({ cfg: { ...s.cfg, ...p } })),
      resetConfig: () => set({ cfg: DEFAULT_T_CONFIG }),
      setShowAdvanced: (showAdvanced) => set({ showAdvanced }),
      setActiveTab: (activeTab) => set({ activeTab }),
      setResult: (result) => set({ result }),
      setScan: (scan) => set({ scan }),
    }),
    {
      name: 'stock-buddy:t-backtest',
      version: 2,
      /**
       * v2 起 `TTrade` 新增 `legs` / `used_position_ratio` 字段。
       * localStorage 里若残留 v1 的回测结果，明细表读 `legs.length` 会直接崩溃，
       * 因此升级时丢弃旧结果（表单参数保留，用户重跑一次即可）。
       */
      migrate: (persisted, version) => {
        const p = (persisted ?? {}) as Partial<TBacktestState>;
        if (version < 2) {
          return { ...p, result: null, scan: null } as TBacktestState;
        }
        return p as TBacktestState;
      },
      /**
       * 配置结构升级后，localStorage 里的旧数据可能缺少新增字段。
       * 这里用默认配置补齐，避免 undefined 让滑块/输入框崩溃；
       * 同时校验回测结果结构，缺字段直接丢弃而不是带病渲染。
       */
      merge: (persisted, current) => {
        const p = (persisted ?? {}) as Partial<TBacktestState>;
        return {
          ...current,
          ...p,
          cfg: { ...DEFAULT_T_CONFIG, ...(p.cfg ?? {}) },
          result: isValidResult(p.result) ? p.result : null,
          scan: isValidScan(p.scan) ? p.scan : null,
        };
      },
    },
  ),
);
