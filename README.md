# Stock-Assister

面向 A 股市场的智能投资辅助系统，采用 Rust 后端 + React 前端的前后端分离架构。提供行情追踪、技术分析、AI 研判、买卖信号检测等全链路辅助能力。

## 功能总览

### 已实现

| 功能模块 | 说明 |
|---------|------|
| **大盘总览** | 上证/深证/创业板三大指数实时卡片，涨跌家数统计，行业/概念板块排行 |
| **板块成分股** | 板块可展开查看成分股列表，点击跳转个股分析页面 |
| **个股分析** | 实时行情面板、多周期 K 线图（日/周/月/分钟）、支撑压力位展示 |
| **买卖信号检测** | 6 维度信号分析（MACD/KDJ/RSI/布林带/量价/支撑压力），综合评分与操作建议 |
| **技术指标引擎** | MACD、KDJ、RSI（6/12/24）、布林带、支撑压力位，纯 Rust 实现 |
| **AI 个股诊断** | 基于 LLM 的个股综合诊断，流式 Markdown 渲染输出 |
| **AI 市场研判** | 市场情绪定调、指数分析、板块轮动分析、操作策略建议 |
| **AI 趋势预测** | 短/中/长期趋势预测，含关键价位与概率研判 |
| **AI 智能问答** | 对话式交互，自动识别股票代码并注入实时数据上下文 |
| **自选股管理** | 添加/移除自选股，SQLite 持久化 |
| **实时行情推送** | WebSocket 每 5 秒轮询推送实时行情 |
| **资金流向分析** | 北向资金趋势、融资融券余额、行业板块资金流排行、个股资金流查询 |
| **涨停复盘** | 涨停/跌停统计、连板梯队、炸板率、一字板/T字板识别 |
| **龙虎榜** | 上榜列表、净买入排行、机构净买入、上榜原因分类 |
| **财务分析** | 利润表/资产负债表核心指标、营收/利润/ROE 趋势图、同比增速 |

### 规划中

- 告警系统（价格突破/均线穿越/涨跌幅阈值）
- 涨停复盘与龙虎榜
- 财务数据与估值分析
- 告警系统（价格突破/均线穿越/涨跌幅阈值）
- 定时任务调度（盘前准备/盘中扫描/盘后复盘）
- Redis 缓存层
- Docker 部署

## 系统架构

```
┌─────────────────────────────────────────────────┐
│              Frontend (React 18 + TS)            │
│         Ant Design / ECharts / Zustand           │
├─────────────────────────────────────────────────┤
│              Vite Proxy (:5173 → :8080)          │
├─────────────────────────────────────────────────┤
│              API Layer (Axum 0.8)                │
│   REST Endpoints    │    WebSocket    │   SSE    │
├─────────────────────────────────────────────────┤
│              Business Analysis Layer             │
│  ┌──────────┐ ┌──────────┐ ┌──────────────────┐ │
│  │ Signals  │ │   AI     │ │  Indicators      │ │
│  │ (6-dim)  │ │ (LLM)    │ │ MACD/KDJ/RSI/... │ │
│  └──────────┘ └──────────┘ └──────────────────┘ │
├─────────────────────────────────────────────────┤
│              Data Fetching Layer                 │
│  ┌────────────────┐  ┌──────────────────┐       │
│  │ Eastmoney API  │  │  Sina API (备用)  │       │
│  └────────────────┘  └──────────────────┘       │
├─────────────────────────────────────────────────┤
│              Storage Layer (SQLite)              │
└─────────────────────────────────────────────────┘
```

### Crate 依赖关系

```
appcore (核心模型与公共类型)
  ├── fetcher (数据采集：东方财富 / 新浪)
  ├── indicators (技术指标计算引擎)
  ├── storage (SQLite 数据库)
  ├── scheduler (定时任务，待完善)
  ├── analyzer (业务分析：信号检测 + AI 服务)
  └── api (HTTP/WS 服务入口，依赖以上全部)
```

## 技术栈

### 后端

| 组件 | 技术 |
|------|------|
| 语言 | Rust (Edition 2021) |
| 异步运行时 | Tokio |
| Web 框架 | Axum 0.8 (REST + WebSocket + SSE) |
| 数据库 | SQLite (sqlx 0.8) |
| HTTP 客户端 | reqwest 0.12 |
| LLM 集成 | OpenAI 兼容 API (DeepSeek / 通义千问 / Moonshot 等) |
| 日志 | tracing + tracing-subscriber |

### 前端

| 组件 | 技术 |
|------|------|
| 框架 | React 18 + TypeScript |
| 构建工具 | Vite 5 |
| UI 组件库 | Ant Design 5 |
| 图表 | ECharts 5 + echarts-for-react |
| 状态管理 | Zustand |
| 路由 | React Router DOM 6 |
| Markdown | react-markdown + remark-gfm |

## 项目结构

```
stock-assister/
├── Cargo.toml                  # Workspace 配置
├── .env                        # 环境变量
├── .env.example                # 环境变量模板
├── crates/
│   ├── appcore/                # 核心模型与公共类型
│   │   └── src/models/         # Stock, Quote, Kline, Market
│   ├── fetcher/                # 数据采集层
│   │   └── src/
│   │       ├── traits.rs       # DataFetcher trait
│   │       ├── eastmoney.rs    # 东方财富适配器
│   │       └── sina.rs         # 新浪财经适配器（备用）
│   ├── indicators/             # 技术指标引擎
│   │   └── src/
│   │       ├── macd.rs         # MACD 计算
│   │       ├── kdj.rs          # KDJ 计算
│   │       ├── rsi.rs          # RSI 计算 (6/12/24)
│   │       ├── bollinger.rs    # 布林带计算
│   │       └── support_resistance.rs  # 支撑压力位
│   ├── analyzer/               # 业务分析层
│   │   └── src/
│   │       ├── signals.rs      # 买卖信号分析（6维度）
│   │       └── ai/             # AI 辅助分析
│   │           ├── llm.rs      # LLM 客户端 (SSE流式)
│   │           ├── prompts.rs  # Prompt 模板
│   │           ├── collector.rs # 数据采集管道
│   │           └── service.rs  # AI 服务门面
│   ├── storage/                # 数据持久化
│   │   └── src/
│   │       ├── database.rs     # SQLite 连接池
│   │       └── repository.rs   # CRUD 操作
│   ├── scheduler/              # 定时任务（待完善）
│   └── api/                    # Web API 入口
│       └── src/
│           ├── main.rs         # 启动入口
│           ├── app.rs          # 应用组装
│           ├── state.rs        # 全局状态
│           └── routes/         # 路由模块
│               ├── quotes.rs   # 行情接口
│               ├── kline.rs    # K线接口
│               ├── market.rs   # 市场接口
│               ├── watchlist.rs # 自选股接口
│               ├── analysis.rs # 信号分析接口
│               ├── ai.rs       # AI 接口
│               └── ws.rs       # WebSocket 接口
├── frontend/                   # React 前端
│   ├── src/
│   │   ├── App.tsx             # 路由配置
│   │   ├── pages/              # 页面组件
│   │   │   ├── Dashboard.tsx   # 大盘总览
│   │   │   ├── StockAnalysis.tsx # 个股分析
│   │   │   ├── AiDiagnosis.tsx # AI 诊断
│   │   │   ├── AiMarket.tsx    # AI 市场研判
│   │   │   └── AiChat.tsx      # AI 问答
│   │   ├── components/         # 通用组件
│   │   │   └── KLineChart.tsx  # K线图组件
│   │   ├── stores/             # 状态管理
│   │   └── api/                # API 封装
│   └── vite.config.ts          # Vite 配置（含代理）
├── migrations/
│   └── 001_init.sql            # 数据库初始化脚本
└── docs/
    └── design-plan.html        # 设计方案文档
```

## 快速开始

### 环境要求

- **Rust** 1.80+
- **Node.js** 18+（推荐 22）
- **npm** 或 **pnpm**

### 1. 配置环境变量

```bash
cp .env.example .env
```

编辑 `.env` 文件，配置关键参数：

```env
LISTEN_ADDR=0.0.0.0:8080     # API 服务监听地址
DB_PATH=data/stock.db         # SQLite 数据库路径
RUST_LOG=info,stock_assister=debug

# LLM 配置（可选，不配置则 AI 功能不可用）
LLM_API_KEY=your-api-key
LLM_BASE_URL=https://api.openai.com/v1
LLM_MODEL=gpt-4o-mini
```

LLM 支持所有 OpenAI 兼容接口，可直接使用 DeepSeek、通义千问、Moonshot 等国产模型。

### 2. 启动后端

```bash
# 方式一：编译后直接运行（推荐，避免网络沙箱问题）
cargo build
./target/debug/stock-assister

# 方式二：cargo run（如遇网络错误请用方式一）
cargo run
```

首次编译约 3-5 分钟，后续增量编译很快。后端启动后监听 `http://localhost:8080`。

### 3. 启动前端

```bash
cd frontend
npm install    # 首次运行需要安装依赖
npm run dev
```

前端开发服务器启动在 `http://localhost:5173`，API 请求自动代理到后端。

### 4. 访问

浏览器打开 **http://localhost:5173**，即可使用全部功能。

## API 接口

所有 API 前缀: `/api/v1`

### 行情

```
GET  /api/v1/quotes/{code}           # 个股实时行情
GET  /api/v1/quotes?codes=sh600000,sz000001  # 批量行情
```

### K 线

```
GET  /api/v1/kline/{code}?period=daily&limit=365
# period: daily / weekly / monthly / 5min / 15min / 30min / 60min
```

### 市场

```
GET  /api/v1/market/overview          # 大盘总览（三大指数 + 涨跌家数）
GET  /api/v1/market/sectors?type=industry   # 板块排行（industry/concept）
GET  /api/v1/market/sectors/{code}/stocks?page=1&size=50  # 板块成分股
```

### 分析

```
GET  /api/v1/analysis/signals/{code}?period=daily&limit=120  # 买卖信号分析
```

### AI

```
GET  /api/v1/ai/status                   # AI 服务状态
GET  /api/v1/ai/diagnose/{code}          # 个股诊断（同步）
GET  /api/v1/ai/diagnose/{code}/stream   # 个股诊断（SSE 流式）
GET  /api/v1/ai/market                   # 市场研判（同步）
GET  /api/v1/ai/market/stream            # 市场研判（SSE 流式）
GET  /api/v1/ai/predict/{code}           # 趋势预测（同步）
GET  /api/v1/ai/predict/{code}/stream    # 趋势预测（SSE 流式）
POST /api/v1/ai/chat                     # 智能问答
POST /api/v1/ai/chat/stream              # 智能问答（SSE 流式）
```

### 自选股

```
GET    /api/v1/watchlist          # 获取自选股列表
POST   /api/v1/watchlist          # 添加自选股 {code, group?}
DELETE /api/v1/watchlist/{code}   # 移除自选股
```

### WebSocket

```
WS /ws/quotes?codes=sh600000,sz000001  # 实时行情推送（每 5 秒）
```

## 数据来源

- **主要数据源**: 东方财富 Push2 API (`push2.eastmoney.com`)
- **备用数据源**: 新浪财经 (`hq.sinajs.cn`)
- **K 线周期**: 日线 / 周线 / 月线 / 1分钟 / 5分钟 / 15分钟 / 30分钟 / 60分钟
- **板块类型**: 行业板块 / 概念板块

> **免责声明**: 本项目仅用于学习研究，不构成任何投资建议。股市有风险，投资需谨慎。

## 买卖信号体系

系统综合 6 个维度检测买卖信号，每个维度的信号按强弱分级：

| 维度 | 买入信号 | 卖出信号 |
|------|---------|---------|
| MACD | 金叉 / 底背离 | 死叉 / 顶背离 |
| KDJ | K上穿D（超卖区为强信号） | K下穿D（超买区为强信号） |
| RSI | 超卖区(<30)回升 | 超买区(>70)回落 |
| 布林带 | 跌破下轨后回升 | 突破上轨后回落 |
| 量价关系 | 放量上涨 / 缩量下跌 | — |
| 支撑压力 | 接近支撑位(<2%) | 接近压力位(<2%) |

**综合评分**: -10 ~ +10，映射到 5 档操作建议（强烈买入 / 偏多 / 中性观望 / 偏空 / 强烈卖出）。

## 开发路线

| 阶段 | 内容 | 状态 |
|------|------|------|
| Phase 1 | 项目骨架、数据采集、SQLite、REST API、React 前端 | ✅ 完成 |
| Phase 2 | 技术指标引擎、买卖信号分析、支撑压力位 | ✅ 完成 |
| Phase 3 | AI 辅助分析（诊断/研判/预测/问答）、SSE 流式 | ✅ 完成 |
| Phase 4 | 资金流向、涨停复盘、龙虎榜、财务数据 | ✅ 完成 |
| Phase 5 | 告警系统、定时任务调度、Redis 缓存 | 🚧 规划中 |
| Phase 6 | Docker 部署、性能优化、生产就绪 | 🚧 规划中 |

## License

MIT
