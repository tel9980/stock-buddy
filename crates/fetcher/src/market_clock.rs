//! A 股交易时段判定与建议轮询间隔。
//!
//! 设计参考了 quant-desktop 的 MarketSession 思路（市场时段 → 分级轮询间隔），
//! 独立实现；固定 UTC+8 判定，不依赖宿主机时区。

use chrono::{DateTime, FixedOffset, Datelike, NaiveTime, Utc, Weekday};

/// A 股交易时段（固定按东八区判定，节假日不在此处理，由上游探测降频兜底）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketSession {
    /// 9:30 前 —— 盘前
    PreOpen,
    /// 9:30–11:30 —— 早盘
    MorningTrade,
    /// 11:30–13:00 —— 午休
    LunchBreak,
    /// 13:00–15:00 —— 午盘
    AfternoonTrade,
    /// 15:00 后 / 周末 —— 休市
    Closed,
}

impl MarketSession {
    /// 当前 A 股时段。固定 UTC+8，与宿主 TZ 无关。
    pub fn now() -> Self {
        let cst: FixedOffset = FixedOffset::east_opt(8 * 3600).expect("UTC+8 valid");
        let now: DateTime<Utc> = Utc::now();
        Self::from_datetime(now.with_timezone(&cst))
    }

    /// 从指定东八区时间判定时段（供测试注入）。
    pub fn from_datetime(cst: DateTime<FixedOffset>) -> Self {
        match cst.weekday() {
            Weekday::Sat | Weekday::Sun => return Self::Closed,
            _ => {}
        }
        let t = cst.time();
        let (t0930, t1130, t1300, t1500) = (
            NaiveTime::from_hms_opt(9, 30, 0).expect("valid"),
            NaiveTime::from_hms_opt(11, 30, 0).expect("valid"),
            NaiveTime::from_hms_opt(13, 0, 0).expect("valid"),
            NaiveTime::from_hms_opt(15, 0, 0).expect("valid"),
        );
        if t < t0930 {
            Self::PreOpen
        } else if t < t1130 {
            Self::MorningTrade
        } else if t < t1300 {
            Self::LunchBreak
        } else if t < t1500 {
            Self::AfternoonTrade
        } else {
            Self::Closed
        }
    }

    /// 是否交易时段（早盘/午盘）。
    pub fn is_trading(self) -> bool {
        matches!(self, Self::MorningTrade | Self::AfternoonTrade)
    }

    /// 个股/聚合数据建议轮询间隔（秒）。聚合类接口有频控，间隔刻意更慢。
    pub fn poll_interval(self, aggregated: bool) -> u64 {
        if aggregated {
            return match self {
                Self::MorningTrade | Self::AfternoonTrade | Self::PreOpen => 60,
                Self::LunchBreak => 120,
                Self::Closed => 300,
            };
        }
        match self {
            Self::MorningTrade | Self::AfternoonTrade => 3,
            Self::PreOpen => 5,
            Self::LunchBreak => 30,
            Self::Closed => 300,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::PreOpen => "盘前",
            Self::MorningTrade => "早盘",
            Self::LunchBreak => "午休",
            Self::AfternoonTrade => "午盘",
            Self::Closed => "休市",
        }
    }
}

/// 三态自适应轮询状态机：开市探测（probe）→ 正常（normal）→ 闲置降频（idle）。
///
/// 进入交易时段后先探测若干次；若连续 `IDLE_AFTER_NO_CHANGE` 次数据无变化
/// （节假日/停牌），降为 `idle` 的超长间隔；一旦有变化立即恢复 normal。
#[derive(Debug)]
pub struct AdaptivePoller {
    session_entered: bool,
    probes_done: u32,
    no_change_streak: u32,
    idle: bool,
}

const PROBE_COUNT: u32 = 3;
const IDLE_AFTER_NO_CHANGE: u32 = 10;
const PROBE_INTERVAL_SECS: u64 = 2;

impl AdaptivePoller {
    pub fn new() -> Self {
        Self { session_entered: false, probes_done: 0, no_change_streak: 0, idle: false }
    }

    /// 每次轮询结束时调用：传入当前时段与「本轮数据是否有变化」，
    /// 返回下一轮轮询间隔（秒）。
    pub fn next_interval(&mut self, session: MarketSession, changed: bool) -> u64 {
        // 离开交易时段：直接按休市间隔
        if !session.is_trading() {
            self.session_entered = false;
            self.idle = false;
            self.no_change_streak = 0;
            return session.poll_interval(false);
        }

        // 刚进入交易时段：开始探测
        if !self.session_entered {
            self.session_entered = true;
            self.probes_done = 0;
            self.idle = false;
            self.no_change_streak = 0;
            return PROBE_INTERVAL_SECS;
        }

        // 探测阶段
        if self.probes_done < PROBE_COUNT {
            self.probes_done += 1;
            if changed {
                self.no_change_streak = 0;
                return session.poll_interval(false); // 确认开市
            }
            if self.probes_done >= PROBE_COUNT {
                // 探测全无变化，疑似节假日 → 直接进 idle
                self.idle = true;
                return session.poll_interval(true).max(300);
            }
            return PROBE_INTERVAL_SECS;
        }

        // 正常/idle 阶段
        if changed {
            self.no_change_streak = 0;
            self.idle = false;
        } else {
            self.no_change_streak += 1;
            if self.no_change_streak >= IDLE_AFTER_NO_CHANGE {
                self.idle = true;
            }
        }
        if self.idle {
            session.poll_interval(true).max(300)
        } else {
            session.poll_interval(false)
        }
    }

    pub fn is_idle(&self) -> bool {
        self.idle
    }
}

impl Default for AdaptivePoller {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn cst(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<FixedOffset> {
        let off = FixedOffset::east_opt(8 * 3600).unwrap();
        off.with_ymd_and_hms(y, m, d, h, mi, 0).unwrap()
    }

    #[test]
    fn sessions() {
        assert_eq!(MarketSession::from_datetime(cst(2026, 10, 26, 9, 0)), MarketSession::PreOpen);
        assert_eq!(MarketSession::from_datetime(cst(2026, 10, 26, 10, 0)), MarketSession::MorningTrade);
        assert_eq!(MarketSession::from_datetime(cst(2026, 10, 26, 12, 0)), MarketSession::LunchBreak);
        assert_eq!(MarketSession::from_datetime(cst(2026, 10, 26, 14, 0)), MarketSession::AfternoonTrade);
        assert_eq!(MarketSession::from_datetime(cst(2026, 10, 26, 16, 0)), MarketSession::Closed);
        assert_eq!(MarketSession::from_datetime(cst(2026, 10, 24, 10, 0)), MarketSession::Closed); // 周六
    }

    #[test]
    fn poll_intervals() {
        assert_eq!(MarketSession::MorningTrade.poll_interval(false), 3);
        assert_eq!(MarketSession::Closed.poll_interval(false), 300);
        assert_eq!(MarketSession::Closed.poll_interval(true), 300);
        assert_eq!(MarketSession::LunchBreak.poll_interval(true), 120);
    }

    #[test]
    fn adaptive_goes_idle_without_changes() {
        let mut p = AdaptivePoller::new();
        let s = MarketSession::MorningTrade;
        // 进入时段 → 探测
        assert_eq!(p.next_interval(s, false), PROBE_INTERVAL_SECS);
        assert_eq!(p.next_interval(s, false), PROBE_INTERVAL_SECS);
        assert_eq!(p.next_interval(s, false), PROBE_INTERVAL_SECS);
        // 探测期结束仍无变化 → idle
        assert_eq!(p.next_interval(s, false), 300);
        assert!(p.is_idle());
        // 有变化 → 恢复正常
        assert_eq!(p.next_interval(s, true), 3);
        assert!(!p.is_idle());
        // 连续 10 次无变化 → idle
        for _ in 0..10 {
            p.next_interval(s, false);
        }
        assert!(p.is_idle());
        // 休市 → 间隔 300
        assert_eq!(p.next_interval(MarketSession::Closed, false), 300);
    }
}
