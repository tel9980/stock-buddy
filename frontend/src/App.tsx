import { useEffect } from 'react'
import { Routes, Route, NavLink } from 'react-router-dom'
import Dashboard from './pages/Dashboard'
import StockAnalysis from './pages/StockAnalysis'
import AiDiagnosis from './pages/AiDiagnosis'
import AiMarket from './pages/AiMarket'
import AiChat from './pages/AiChat'
import AiHistory from './pages/AiHistory'
import CapitalFlow from './pages/CapitalFlow'
import LimitUpReview from './pages/LimitUpReview'
import DragonTiger from './pages/DragonTiger'
import FinancialAnalysis from './pages/FinancialAnalysis'
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
        <div className="logo">Stock-Assister</div>
        <nav>
          <NavLink to="/" end>大盘总览</NavLink>
          <NavLink to="/stock">个股分析</NavLink>
          <NavLink to="/capital">资金流向</NavLink>
          <NavLink to="/limitup">涨停复盘</NavLink>
          <NavLink to="/dragontiger">龙虎榜</NavLink>
          <NavLink to="/ai/diagnosis">AI 诊断</NavLink>
          <NavLink to="/ai/market">AI 研判</NavLink>
          <NavLink to="/ai/chat">AI 问答</NavLink>
          <NavLink to="/ai/history">AI 历史</NavLink>
        </nav>
        <WatchlistMenu />
      </header>
      <main className="app-content">
        <Routes>
          <Route path="/" element={<Dashboard />} />
          <Route path="/stock" element={<StockAnalysis />} />
          <Route path="/capital" element={<CapitalFlow />} />
          <Route path="/limitup" element={<LimitUpReview />} />
          <Route path="/dragontiger" element={<DragonTiger />} />
          <Route path="/finance" element={<FinancialAnalysis />} />
          <Route path="/ai/diagnosis" element={<AiDiagnosis />} />
          <Route path="/ai/market" element={<AiMarket />} />
          <Route path="/ai/chat" element={<AiChat />} />
          <Route path="/ai/history" element={<AiHistory />} />
        </Routes>
      </main>
    </div>
  )
}
