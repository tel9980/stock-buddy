import { useCallback } from 'react';
import { create } from 'zustand';
import { persist } from 'zustand/middleware';

/**
 * 通用的「页面表单状态持久化」。
 *
 * 背景：React Router 切换菜单会**卸载**页面组件，组件内的 useState 随之销毁，
 * 用户填好的查询条件、参数、筛选器全部丢失。这里用一个持久化的 key-value 袋，
 * 各页面只需把 `useState(x)` 换成 `usePersistedState('页面.字段', x)` 即可无感保留，
 * 刷新浏览器也不丢。
 *
 * 使用建议：
 * - 只持久化**输入类**状态（输入框、选择器、开关、筛选条件）。
 * - 不要持久化 loading / error / 会过期的结果数据。
 */

type Bag = Record<string, unknown>;

interface PersistedState {
  bag: Bag;
  setValue: (key: string, value: unknown) => void;
  clearAll: () => void;
}

export const usePersistedStore = create<PersistedState>()(
  persist(
    (set) => ({
      bag: {},
      setValue: (key, value) => set((s) => ({ bag: { ...s.bag, [key]: value } })),
      clearAll: () => set({ bag: {} }),
    }),
    {
      name: 'stock-buddy:page-state',
      version: 1,
    },
  ),
);

/**
 * 与 `useState` 同签名的持久化版本。
 *
 * @param key 全局唯一键，约定用 `页面.字段` 命名，如 `'calc.price'`
 */
export function usePersistedState<T>(key: string, initial: T): [T, (v: T) => void] {
  const stored = usePersistedStore((s) => s.bag[key]);
  const setValue = usePersistedStore((s) => s.setValue);

  const value = (stored === undefined ? initial : stored) as T;
  const set = useCallback(
    (v: T) => {
      setValue(key, v);
    },
    [key, setValue],
  );

  return [value, set];
}
