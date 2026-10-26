import { useEffect } from 'react'
import { Routes, Route, NavLink } from 'react-router-dom'
import { lazy, Suspense } from 'react'
const Dashboard = lazy(() => import('./pages/Dashboard'))
const StockAnalysis = lazy(() => import('./pages/StockAnalysis'))
const AiDiagnosis = lazy(() => import('./pages/AiDiagnosis'))
const AiMarket = lazy(() => import('./pages/AiMarket'))
const AiChat = lazy(() => import('./pages/AiChat'))
const AiHistory = lazy(() => import('./pages/AiHistory'))
const CapitalFlow = lazy(() => import('./pages/CapitalFlow'))
const SectorHotspot = lazy(() => import('./pages/SectorHotspot'))
const LimitUpReview = lazy(() => import('./pages/LimitUpReview'))
const DragonTiger = lazy(() => import('./pages/DragonTiger'))
const FinancialAnalysis = lazy(() => import('./pages/FinancialAnalysis'))
const PriceChangeCalculator = lazy(() => import('./pages/PriceChangeCalculator'))
const TCostOptimizer = lazy(() => import('./pages/TCostOptimizer'))
const TBacktest = lazy(() => import('./pages/TBacktest'))
const MarketSentiment = lazy(() => import('./pages/MarketSentiment'))
const Settings = lazy(() => import('./pages/Settings'))
import WatchlistMenu from './components/WatchlistMenu'
import { useStockStore } from './stores/stockStore'
import './index.css'

export default function App() {
  const fetchWatchlist = useStockStore((s) => s.fetchWatchlist)

  useEffect(() => {
    fetchWatchlist()
  }, [fetchWatchlist])

  return (
    <div className="app-layout">
      <header className="app-header">
        <div className="logo">Stock Buddy</div>
        <nav>
          <NavLink to="/" end>大盘总览</NavLink>
          <NavLink to="/stock">个股分析</NavLink>
          <NavLink to="/capital">资金流向</NavLink>
          <NavLink to="/limitup">涨停复盘</NavLink>
          <NavLink to="/dragontiger">龙虎榜</NavLink>
          <NavLink to="/sector">市场热点</NavLink>
          <NavLink to="/sentiment">情绪日报</NavLink>
          <NavLink to="/calc">涨幅计算</NavLink>
          <NavLink to="/tcalc">做T优化</NavLink>
          <NavLink to="/tbacktest">做T回测</NavLink>
          <NavLink to="/ai/diagnosis">AI 诊断</NavLink>
          <NavLink to="/ai/market">AI 研判</NavLink>
          <NavLink to="/ai/chat">AI 问答</NavLink>
          <NavLink to="/ai/history">AI 历史</NavLink>
          <NavLink to="/settings">设置</NavLink>
        </nav>
        <WatchlistMenu />
      </header>
      <main className="app-content">
        <Suspense fallback={<div style={{ textAlign: 'center', padding: 80 }}>加载中…</div>}>
        <Routes>
          <Route path="/" element={<Dashboard />} />
          <Route path="/stock" element={<StockAnalysis />} />
          <Route path="/capital" element={<CapitalFlow />} />
          <Route path="/limitup" element={<LimitUpReview />} />
          <Route path="/dragontiger" element={<DragonTiger />} />
          <Route path="/sector" element={<SectorHotspot />} />
          <Route path="/sentiment" element={<MarketSentiment />} />
          <Route path="/finance" element={<FinancialAnalysis />} />
          <Route path="/calc" element={<PriceChangeCalculator />} />
          <Route path="/tcalc" element={<TCostOptimizer />} />
          <Route path="/tbacktest" element={<TBacktest />} />
          <Route path="/ai/diagnosis" element={<AiDiagnosis />} />
          <Route path="/ai/market" element={<AiMarket />} />
          <Route path="/ai/chat" element={<AiChat />} />
          <Route path="/ai/history" element={<AiHistory />} />
          <Route path="/settings" element={<Settings />} />
        </Routes>
        </Suspense>
      </main>
    </div>
  )
}
