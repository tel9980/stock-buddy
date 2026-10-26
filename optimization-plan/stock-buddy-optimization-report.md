# Stock Buddy 优化方案（修订版 v2）

> 原报告文件丢失后基于审查上下文重建，并记录本轮已实际落地的修复。
> 生成时间：2026-10-26。本轮已用 `npm run build` 验证前端；`cargo` 不可用，Rust 侧改动仅静态核对。

## 一、本轮已完成的修复（代码已改，编译验证状态见括号）

### 阶段 1：编译级 + 数据源 + 指标
| # | 问题 | 位置 | 修复 |
|---|------|------|------|
| C1 | 已提交代码无法编译（语法错误+未定义类型） | `crates/api/src/main.rs:16-18` | 删除坏代码及无用 `use HashMap` ✅ |
| C2 | 全仓 HTTP 客户端零超时（一次挂起占死并发槽） | `eastmoney.rs:158` / `sina.rs:15` | 加 `connect_timeout(3s)` + `timeout(10s)` ✅（静态） |
| C3 | `d[..10]` 裸切片，上游日期变短即 panic | `eastmoney.rs:1719/1826` | 统一为 `if d.len() >= 10 { … } else { d }`（与 ：2237 对齐）✅（静态） |
| C4 | `now_ts()` 时钟倒退 panic | `eastmoney.rs:177-182` | `unwrap_or(0)` ✅（静态） |
| C5 | 新浪 `text[i..]` 把序号当字节偏移，遇中文 panic 且 O(n²) | `sina.rs:128-134` | 改 `text.find(var_name)` 绝对偏移 ✅（静态） |
| C6 | KDJ `hh==ll` 时 RSV 强制 0（误判超卖） | `kdj.rs:33` | 中性值 50 + 浮点容差 ✅（静态） |
| C7 | MACD ×2 系数 | `macd.rs:42` | **保留 ×2**（国内行情软件惯例口径，仅常数倍差；未参与信号强度计算），加注释说明 |
| S6 | `save_klines` 死代码且逐条 INSERT 无事务 | `repository.rs:104-128` | 删除，留迁移注释 ✅（静态） |

### 阶段 2：回测 + 前端
| # | 问题 | 位置 | 修复 |
|---|------|------|------|
| B1 | 未来函数：信号第 i 根收盘生成却用同根收盘价成交 | `backtest.rs:29-34` | 执行统一顺延到第 i+1 根开盘价；末根信号丢弃 ✅（静态） |
| F1 | ECharts 实例泄漏 + 原地 reverse 污染 props | `CapitalFlow.tsx` | ref 持有实例、卸载 dispose、复用 init；`[...nb].reverse()` ✅（构建通过） |
| F2 | AI 流式不可中断 | `AiDiagnosis.tsx` + `api/index.ts` | `AbortController` 贯穿 3 个 GET 流 + 卸载自动中断 + 「停止生成」按钮 ✅（构建通过） |
| F3 | SSE 中断被当错误 | `api/index.ts streamSSE` | AbortError → 走 onDone；全部流接口加 `signal` 参数 ✅（构建通过） |
| F4 | 自选股写接口静默失败 | `stockStore.ts` / `WatchlistMenu.tsx` | 检查 `resp.ok` 并抛错/提示 ✅（构建通过） |
| F5 | MarkdownView 放开 `style` = CSS 注入 | `MarkdownView.tsx` | 白名单仅保留 `className`，style 一律拦截（LLM 输出不可信）✅（构建通过） |
| F6 | 龙虎榜名称列 O(n²) filter | `MarketSentiment.tsx` | 预建 `Map<code,count>` 索引 ✅（构建通过） |

### 阶段 3：打包 + 仓库卫生
| # | 修复 |
|---|------|
| P1 | App 全部页面改 `React.lazy` + `Suspense`（首屏只载 Dashboard chunk）✅ |
| P2 | `vite.config.ts` 加 `manualChunks`：antd / echarts / react-markdown 各自独立分包，业务改动不再打爆 vendor 缓存 ✅ |
| P3 | `.gitignore` 补 `*.tsbuildinfo` ✅ |

**构建验证**：`npm install && npm run build` 通过（tsc + vite，页面按路由分包成功）。

## 二、后续待办（未在本轮落地，按优先级）

### 高优先级（安全/稳定性）
1. **鉴权 + 限流 + 超时三件套**（API 层 #2/#3/#4/#5/#13）：`LISTEN_ADDR` 默认 `127.0.0.1`、Bearer 中间件、CORS 收敛 origin、`TimeoutLayer(30s)`、sentiment 全市场抓取全局 `Semaphore(1)` + 同 date single-flight、AI 端点限流。
2. **调度器加固**（API 层 #6/#14）：`catch_unwind` 包 `job.run`、`running` key 改 RAII 清理、每 job `tokio::time::timeout` 预算、时区钉 `chrono_tz::Asia::Shanghai`。
3. **统一错误处理**（API 层 #7/#8/#19）：`error.rs: enum AppError` + `IntoResponse`，删 14 份复制 err()；对外固定文案+trace_id，不再透出内部错误。
4. **输入校验**（API 层 #11/#12）：`code` 路径参数 `^[0-9]{6}$` 校验；分页 size clamp。
5. **重试/限流下沉数据源层**（fetcher 层 S2/S3/M3）：`em_get` 统一封装（状态码判定 + 5xx 退避重试 3 次 + 单主机 10req/s 令牌桶）；熔断器补 half-open。
6. **涨停/炸板/北向口径修正**（fetcher 层 S5/M4/M5）：±9.5% 硬编码 → 按板块限幅；炸板判据改「触涨停价且收盘<涨停价」；北向单边缺失不再当 0。

### 中优先级（性能/正确性）
7. WS 行情聚合广播（N 连接 ≠ N 倍上游请求）+ codes 上限 + 心跳。
8. `prune_daily_bars` 改 `date < cutoff` 索引删除 + 单事务；批量写分块 commit；`busy_timeout(30s)`。
9. 迁移改 `sqlx::migrate!`（版本表 + 不再 `.ok()` 吞错）。
10. `fetch_quotes` 批量化（东财 ulist.np 支持多 secid）。
11. `secid` 920 段北交所前缀修正（复用 `tencent_secid` 规则）。
12. 时区统一（采集/存储/调度三处基准统一到 Asia/Shanghai）。
13. sentiment 导出 `std::fs::write` → `tokio::fs::write`。

### 低优先级（可维护性）
14. `eastmoney.rs`（2791 行）拆 `em/{clist,tencent,sina,datacenter}.rs`；sentiment 路由拆 handler/fetch/aggregate/cache/export。
15. 死代码清理（`fetch_market_overview` 的 `_sh_url/_sz_url`、overview 硬编码 0 改真实聚合）。
16. CI：GitHub Actions 跑 `cargo test` + `npm run build`。

## 三、验证清单（环境恢复后执行）
```bash
cargo build --workspace          # C1/C2/C3/C4/C5/C6/S6/B1 的编译验证
cargo test  --workspace          # kdj/macd/backtest 既有单测
cargo clippy --workspace         # 静态检查
cd frontend && npm run build     # ✅ 本轮已通过
```
注意：`backtest.rs` 单测若断言成交价=收盘价，需同步改为下一根开盘价口径。
