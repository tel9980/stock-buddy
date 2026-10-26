//! 统一的「市场时钟」：所有日期/时间一律按 **UTC+8（Asia/Shanghai）** 判定。
//!
//! 为什么必须统一：本项目原先混用三套时区来源——
//!
//! | 来源 | 行为 | 问题 |
//! |---|---|---|
//! | `chrono::Local` | 跟随宿主 `TZ` | 容器里 `TZ=Asia/Shanghai` 才对，`TZ` 没设时（如直接跑二进制、或 `docker run` 忘了 `-e TZ`）会拿到 UTC，**日期直接相差 8 小时** |
//! | `chrono::Utc` | 恒定 UTC | 与 `Local` 混用时，`Utc::now()` 在 16:00 UTC 之前算出来的「今天」比东八区晚一天 |
//! | `FixedOffset::east(8)` | 恒定东八区 | 正确，但原先只在 `market_clock` 里零散用过 |
//!
//! 混用的实际后果不是「显示差 8 小时」这么轻：
//! - 早盘 08:00–09:30（东八区）之间，UTC 还是前一天，`today` 取到昨日 →
//!   涨停池/资金流按昨天取数，页面数据整体滞后一天；
//! - 反过来的方向，17:00–24:00 东八区时 `Local` 若为 UTC 则还是当天，
//!   而 `Utc` 已经是明天，可能取到「未来日期」查不到数据；
//! - 日报落盘文件名、缓存键、交易日探测相互打架，同一份报告被当成两天的数据。
//!
//! 因此：**新代码一律用本模块**，不要再写 `chrono::Local::now()`。
//!
//! 注意这里不判断节假日（要判断得查交易日历，属于上层职责）；
//! 只保证「是哪一天」「现在几点」在东八区意义下是唯一的。

use chrono::{DateTime, Datelike, Duration, FixedOffset, NaiveDate, TimeZone, Timelike, Utc, Weekday};

/// 东八区固定偏移。A 股所有时间语义都在这个时区下。
pub const CST_OFFSET_SECS: i32 = 8 * 3600;

/// 取东八区偏移量（`FixedOffset` 构造失败会 panic，这里集中一次并给出明确信息）
pub fn cst() -> FixedOffset {
    FixedOffset::east_opt(CST_OFFSET_SECS).expect("UTC+8 是合法固定偏移")
}

/// 当前的东八区时间
pub fn now() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&cst())
}

/// 今天的日期（东八区）
pub fn today() -> NaiveDate {
    now().date_naive()
}

/// 今天的日期字符串 `YYYY-MM-DD`（东八区）
pub fn today_str() -> String {
    today().format("%Y-%m-%d").to_string()
}

/// 当前 Unix 秒
///
/// 时间戳本身与时区无关，但集中到这里可以减少「有人又用一次 `Utc::now()`」的机会。
pub fn now_ts() -> i64 {
    Utc::now().timestamp()
}

/// 当前东八区时间的 `HH:MM:SS`
pub fn now_hms() -> String {
    let n = now();
    format!("{:02}:{:02}:{:02}", n.hour(), n.minute(), n.second())
}

/// 是否周末（东八区）
pub fn is_weekend(d: NaiveDate) -> bool {
    matches!(d.weekday(), Weekday::Sat | Weekday::Sun)
}

/// 最近一个「工作日」日期（东八区，只看周末，不看节假日）。
///
/// 盘前/盘中查一些「按当日聚合」的接口时，日内的集合还没形成，
/// 取当日会得到空数据，用这个回退到上一个工作日更稳妥。
pub fn last_workday(from: NaiveDate) -> NaiveDate {
    let mut d = from;
    while is_weekend(d) {
        d -= Duration::days(1);
    }
    d
}

/// 指定日期东八区零点对应的毫秒时间戳。
///
/// 上游不少接口（同花顺涨停池等）要求「交易日上海时区零点的毫秒戳」。
pub fn day_start_ms(d: NaiveDate) -> i64 {
    cst()
        .from_local_datetime(&d.and_hms_opt(0, 0, 0).expect("00:00:00 合法"))
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

/// 从毫秒时间戳取它对应的东八区日期
pub fn date_of_ms(ms: i64) -> Option<NaiveDate> {
    Utc.timestamp_millis_opt(ms)
        .single()
        .map(|dt| dt.with_timezone(&cst()).date_naive())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cst_is_utc_plus_8() {
        assert_eq!(cst().local_minus_utc(), 8 * 3600);
    }

    #[test]
    fn day_start_ms_is_midnight_cst() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 27).unwrap();
        let ms = day_start_ms(d);
        // 东八区 2026-10-27 00:00 == UTC 2026-10-26 16:00 == 1793030400 秒
        assert_eq!(ms, 1_793_030_400_000);
        // 往返一致
        assert_eq!(date_of_ms(ms), Some(d));
    }

    #[test]
    fn last_workday_skips_weekend() {
        // 2026-10-25 是周日 → 回退到 10-23 周五
        let sun = NaiveDate::from_ymd_opt(2026, 10, 25).unwrap();
        assert_eq!(last_workday(sun), NaiveDate::from_ymd_opt(2026, 10, 23).unwrap());
        // 2026-10-24 是周六 → 同样回退到 10-23
        let sat = NaiveDate::from_ymd_opt(2026, 10, 24).unwrap();
        assert_eq!(last_workday(sat), NaiveDate::from_ymd_opt(2026, 10, 23).unwrap());
        // 工作日原样返回
        let tue = NaiveDate::from_ymd_opt(2026, 10, 27).unwrap();
        assert_eq!(last_workday(tue), tue);
    }

    #[test]
    fn utc_date_differs_from_cst_near_midnight() {
        // 这条测试记录的是「为什么不能混用 Utc 与 Local」：
        // UTC 2026-10-26 16:00 == 东八区 2026-10-27 00:00
        let ms = day_start_ms(NaiveDate::from_ymd_opt(2026, 10, 27).unwrap());
        let utc_date = Utc
            .timestamp_millis_opt(ms)
            .single()
            .unwrap()
            .date_naive();
        // 同一时刻，UTC 的日期比东八区**早一天**——若代码用 Utc 取「今天」，早上 8 点前会整体错一天
        assert_eq!(utc_date, NaiveDate::from_ymd_opt(2026, 10, 26).unwrap());
        assert_eq!(date_of_ms(ms), NaiveDate::from_ymd_opt(2026, 10, 27));
    }
}
