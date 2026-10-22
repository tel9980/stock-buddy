//! 市场情绪日报 → 单文件 HTML
//!
//! 所有样式、脚本、图表均为内联生成，产物无任何外部依赖，可离线打开或直接归档。
//! 视觉沿用 `docs/market-sentiment-2026-07-24.html` 的暗色盘面复盘风格。

use std::collections::HashMap;
use std::fmt::Write as _;

use appcore::models::{
    HotTheme, IndustryBoard, SentimentDaily, ThemeStock,
};

const UP: &str = "#ef4444";
const DOWN: &str = "#22c55e";
const DIM: &str = "#9090a8";
const GRID: &str = "#2a2a42";
/// SVG 内文本颜色。单独常量而非内联字面量，是为了避免 `#` 紧跟 `"` 提前
/// 终止 Rust 的 `r#"..."#` 原生字符串——这个坑很隐蔽，编译期才报奇怪的语法错。
/// 主色 `--accent` 定义在下方 CSS 中，此处不重复声明。
const TEXT_C: &str = "#e0e0e8";

/// 样式表。独立为常量而非内嵌 format! 模板，避免大括号转义污染可读性。
const CSS: &str = r#"
:root{
  --bg:#0f0f17; --surface:#1a1a2e; --surface2:#22223a; --border:#2a2a42;
  --text:#e0e0e8; --dim:#9090a8; --up:#ef4444; --down:#22c55e; --accent:#e8804e;
}
*{margin:0;padding:0;box-sizing:border-box}
body{background:var(--bg);color:var(--text);font-size:13px;line-height:1.5;
  font-family:-apple-system,BlinkMacSystemFont,"Segoe UI","PingFang SC",
  "Hiragino Sans GB","Microsoft YaHei",sans-serif;-webkit-font-smoothing:antialiased}
.mono{font-variant-numeric:tabular-nums;
  font-family:"SF Mono",Menlo,Consolas,"Roboto Mono",monospace}
.wrap{max-width:1340px;margin:0 auto;padding:20px 18px 60px}
.up{color:var(--up)}.down{color:var(--down)}.flat{color:var(--dim)}
.strong{font-weight:600}.dim{color:var(--dim)}.sm{font-size:10.5px}

.hd{display:flex;align-items:flex-end;justify-content:space-between;gap:16px;
  flex-wrap:wrap;padding-bottom:14px;border-bottom:1px solid var(--border)}
.hd h1{font-size:20px;font-weight:700;letter-spacing:.5px}
.hd h1 em{color:var(--accent);font-style:normal}
.hd .sub{color:var(--dim);font-size:12px;margin-top:4px}
.idxbar{display:flex;gap:8px;flex-wrap:wrap}
.idx-item{background:var(--surface);border:1px solid var(--border);border-radius:6px;
  padding:6px 10px;display:flex;gap:8px;align-items:baseline}
.idx-name{color:var(--dim);font-size:11px}
.idx-close{font-size:13px;font-weight:600}

.kpis{display:grid;grid-template-columns:repeat(auto-fit,minmax(196px,1fr));
  gap:10px;margin:16px 0}
.kpi{background:var(--surface);border:1px solid var(--border);border-radius:8px;
  padding:12px 14px;position:relative;overflow:hidden}
.kpi::before{content:"";position:absolute;left:0;top:0;bottom:0;width:3px;
  background:var(--accent)}
.kpi.k2::before{background:var(--up)}.kpi.k3::before{background:#f9c74f}
.kpi.k4::before{background:var(--down)}.kpi.k5::before{background:#7c8cff}
.kpi .k-l{color:var(--dim);font-size:11px;letter-spacing:.3px}
.kpi .k-v{font-size:20px;font-weight:700;margin:3px 0;letter-spacing:-.3px}
.kpi .k-s{color:var(--dim);font-size:11px;line-height:1.5}

.tabs{display:flex;gap:4px;margin:18px 0 0;border-bottom:1px solid var(--border);
  flex-wrap:wrap}
.tab{background:none;border:none;color:var(--dim);font-size:13px;font-weight:600;
  padding:10px 18px;cursor:pointer;border-bottom:2px solid transparent;
  font-family:inherit;transition:.15s}
.tab:hover{color:var(--text)}
.tab.active{color:var(--accent);border-bottom-color:var(--accent)}
.tab .badge{display:inline-block;background:var(--surface2);border-radius:9px;
  padding:0 6px;font-size:10px;margin-left:6px;color:var(--dim);font-weight:600}
.panel{display:none;padding-top:16px}.panel.active{display:block}

.sec{background:var(--surface);border:1px solid var(--border);border-radius:8px;
  padding:14px 16px;margin-bottom:14px}
.sec-h{display:flex;align-items:baseline;justify-content:space-between;gap:12px;
  margin-bottom:10px;flex-wrap:wrap}
.sec-h h2{font-size:14px;font-weight:600}
.sec-h .note{color:var(--dim);font-size:11px}
.chart{width:100%;height:auto;display:block;overflow:visible}
.legend{display:flex;gap:14px;color:var(--dim);font-size:11px;margin-top:8px;
  flex-wrap:wrap;align-items:center}
.legend i{display:inline-block;width:9px;height:9px;border-radius:2px;
  margin-right:4px;vertical-align:middle}

.tw{overflow:auto;max-height:660px;border:1px solid var(--border);border-radius:6px}
table{width:100%;border-collapse:collapse;font-size:12px}
thead th{position:sticky;top:0;background:var(--surface2);color:var(--dim);
  font-weight:600;font-size:11px;text-align:right;padding:7px 9px;
  border-bottom:1px solid var(--border);white-space:nowrap;z-index:2}
thead th:nth-child(1),thead th:nth-child(2),thead th:nth-child(3),
thead th.l{text-align:left}
tbody td{padding:5px 9px;text-align:right;border-bottom:1px solid #1e1e30;
  white-space:nowrap}
tbody td.idx{text-align:left;color:var(--dim);font-size:11px;width:32px}
tbody td.code{text-align:left;color:var(--dim);font-size:11px}
tbody td.nm{text-align:left;font-weight:600}
tbody td.nm2{text-align:left}
tbody td.rs{text-align:left;color:var(--dim);font-size:11px;white-space:normal;
  min-width:300px;line-height:1.35}
tbody td.tags{text-align:left;white-space:normal;min-width:270px}
tbody tr:hover td{background:#1f1f33}
tbody tr:nth-child(even) td{background:#171726}
tbody tr:nth-child(even):hover td{background:#1f1f33}
sup.dup{color:var(--accent);font-size:9px;margin-left:2px}

.tag{display:inline-block;background:#24243c;border:1px solid #33334f;
  color:#b9b9d0;border-radius:3px;padding:1px 5px;font-size:10.5px;
  margin:1px 2px 1px 0}
.tgrid{display:grid;grid-template-columns:repeat(auto-fill,minmax(272px,1fr));gap:10px}
.tcard{background:var(--surface);border:1px solid var(--border);border-radius:8px;
  padding:11px 12px}
.tcard-h{display:flex;justify-content:space-between;align-items:baseline;gap:8px}
.tname{font-weight:700;font-size:13px}
.tcount{color:var(--accent);font-weight:700;font-size:12px}
.tmeta{color:var(--dim);font-size:11px;margin:4px 0 6px}
.tmem{display:flex;flex-wrap:wrap;gap:3px}
.mchip{background:#20203a;border-radius:3px;padding:1px 5px;font-size:10.5px;
  display:inline-flex;gap:4px;align-items:baseline}
.mchip b{font-weight:600}.mchip i{font-style:normal;font-size:10px}
.mchip.more{color:var(--dim);background:transparent}

.rr{display:flex;justify-content:space-between;gap:10px;padding:4px 0;
  border-bottom:1px solid #1e1e30;font-size:11.5px}
.rr:last-child{border-bottom:none}
.rr-c{color:var(--accent);font-weight:700;white-space:nowrap}
.cols{display:grid;grid-template-columns:1fr 350px;gap:14px;align-items:start}
@media(max-width:960px){.cols{grid-template-columns:1fr}}

.ft{margin-top:24px;padding-top:14px;border-top:1px solid var(--border);
  color:var(--dim);font-size:11px;line-height:1.8}
.ft b{color:#b9b9d0;font-weight:600}
.ft .src{display:flex;flex-wrap:wrap;gap:6px;margin:6px 0}
.ft .sb{background:var(--surface);border:1px solid var(--border);border-radius:4px;
  padding:2px 7px}
.warn{background:#2a2418;border:1px solid #4a3a1a;border-radius:6px;padding:8px 11px;
  color:#e0c98a;font-size:11px;margin-top:8px;line-height:1.65}
"#;

const SCRIPT: &str = r#"
document.querySelectorAll('.tab').forEach(function(btn){
  btn.addEventListener('click',function(){
    document.querySelectorAll('.tab').forEach(function(b){b.classList.remove('active')});
    document.querySelectorAll('.panel').forEach(function(p){p.classList.remove('active')});
    btn.classList.add('active');
    document.getElementById(btn.dataset.t).classList.add('active');
  });
});
"#;

// ── 格式化工具 ────────────────────────────────────────────────────

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// 金额格式化。项目内金额统一为「万元」，此处转换为中文习惯的 万/亿/万亿。
fn money(wan: f64) -> String {
    money_with_sign(wan, true)
}

/// `abs_label` 为 true 时不带负号（用于「卖出方 X」这类已由文案表达方向的场景）
fn money_with_sign(wan: f64, abs_label: bool) -> String {
    let a = wan.abs();
    let sign = if abs_label || wan >= 0.0 {
        String::new()
    } else {
        "-".to_string()
    };
    if a >= 1e8 {
        format!("{sign}¥{:.2}万亿", a / 1e8)
    } else if a >= 1e4 {
        format!("{sign}¥{:.2}亿", a / 1e4)
    } else {
        format!("{sign}¥{a:.0}万")
    }
}

fn money_opt(wan: Option<f64>) -> String {
    match wan {
        Some(v) => money(v),
        None => "—".to_string(),
    }
}

fn pct(v: f64) -> String {
    format!("{:+.2}%", v)
}

fn cls(v: f64) -> &'static str {
    if v > 0.0 {
        "up"
    } else if v < 0.0 {
        "down"
    } else {
        "flat"
    }
}

fn color_of(v: f64) -> &'static str {
    if v > 0.0 {
        UP
    } else if v < 0.0 {
        DOWN
    } else {
        DIM
    }
}

/// 星期几的中文名
///
/// 用 `%u`（1=周一 … 7=周日）而非 `NaiveDate::weekday()`：后者在当前 chrono
/// 版本中是私有方法，直接调用会编译失败。
fn weekday_cn(date: &str) -> &'static str {
    let names = ["一", "二", "三", "四", "五", "六", "日"];
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.format("%u").to_string().parse::<usize>().ok())
        .and_then(|n| names.get(n.wrapping_sub(1)))
        .copied()
        .unwrap_or("?")
}

fn generated_str(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_else(|| "—".to_string())
}

// ── SVG 图表 ──────────────────────────────────────────────────────

/// 题材热度条形图：条长 = 出现只数，颜色 = 该题材平均涨跌幅
fn svg_theme_bars(items: &[HotTheme], width: u32, row_h: u32, label_w: u32, val_w: u32) -> String {
    let n = items.len() as u32;
    let height = n * row_h + 30;
    let plot_w = width as f64 - label_w as f64 - val_w as f64 - 10.0;
    let mx = items.iter().map(|i| i.count).max().unwrap_or(1).max(1) as f64;

    let mut o = String::new();
    let _ = write!(
        o,
        r#"<svg viewBox="0 0 {width} {height}" xmlns="http://www.w3.org/2000/svg" role="img" class="chart">"#
    );

    for g in 0..5 {
        let x = label_w as f64 + plot_w * g as f64 / 4.0;
        let _ = write!(
            o,
            r#"<line x1="{x:.1}" y1="16" x2="{x:.1}" y2="{y2}" stroke="{GRID}" stroke-width="1"/>"#,
            y2 = height - 14
        );
    }

    for (i, it) in items.iter().enumerate() {
        let y = 16 + i as u32 * row_h;
        let bh = row_h - 9;
        let w = plot_w * it.count as f64 / mx;
        let fill = color_of(it.avg_pct);
        let _ = write!(
            o,
            r#"<text x="{lx}" y="{ty}" fill="{TEXT_C}" font-size="12" text-anchor="end">{tag}</text>"#,
            lx = label_w - 8,
            ty = y + bh - 2,
            tag = esc(&it.tag)
        );
        let _ = write!(
            o,
            r#"<rect x="{lx}" y="{y}" width="{w:.1}" height="{bh}" fill="{fill}" opacity="0.82" rx="2"/>"#,
            lx = label_w,
            w = w.max(1.5)
        );
        let _ = write!(
            o,
            r#"<text x="{tx:.1}" y="{ty}" fill="{TEXT_C}" font-size="11" font-weight="600">{c} 只</text>"#,
            tx = label_w as f64 + w + 7.0,
            ty = y + bh - 2,
            c = it.count
        );
        let _ = write!(
            o,
            r#"<text x="{tx}" y="{ty}" fill="{fill}" font-size="11" text-anchor="end">均{avg}</text>"#,
            tx = width - 8,
            ty = y + bh - 2,
            avg = pct(it.avg_pct)
        );
    }

    o.push_str("</svg>");
    o
}

/// 双向条形图条目
struct Div<'a> {
    /// 左侧标签
    label: &'a str,
    /// 决定条形方向与长度的数值
    value: f64,
    /// 紧跟条形末端的文本（如净买入金额、涨跌幅）
    bar_text: String,
    /// 最右侧的附注（如涨跌幅、涨跌家数）
    note: String,
}

/// 零轴居中的双向条形图，用于净买入排名与行业涨跌幅榜
fn svg_diverging(items: &[Div], width: u32, row_h: u32, label_w: u32, side_w: u32) -> String {
    let n = items.len() as u32;
    let height = n * row_h + 30;
    let plot_w = width as f64 - label_w as f64 - side_w as f64 * 2.0 - 12.0;
    let cx = label_w as f64 + plot_w / 2.0;
    let half = plot_w / 2.0 - 4.0;
    let mx = items
        .iter()
        .map(|i| i.value.abs())
        .fold(0.0f64, f64::max)
        .max(1.0);

    let mut o = String::new();
    let _ = write!(
        o,
        r#"<svg viewBox="0 0 {width} {height}" xmlns="http://www.w3.org/2000/svg" role="img" class="chart">"#
    );

    for g in [-1.0f64, -0.5, 0.0, 0.5, 1.0] {
        let x = cx + half * g;
        let _ = write!(
            o,
            r#"<line x1="{x:.1}" y1="16" x2="{x:.1}" y2="{y2}" stroke="{GRID}" stroke-width="1"/>"#,
            y2 = height - 14
        );
    }

    for (i, it) in items.iter().enumerate() {
        let y = 16 + i as u32 * row_h;
        let bh = row_h - 9;
        let w = (it.value.abs() / mx) * half;
        let fill = color_of(it.value);

        let _ = write!(
            o,
            r#"<text x="{lx}" y="{ty}" fill="{TEXT_C}" font-size="12" text-anchor="end">{label}</text>"#,
            lx = label_w - 8,
            ty = y + bh - 2,
            label = esc(it.label)
        );

        // 正值自零轴向右延伸、负值向左，标签始终贴在条形末端外侧
        let _ = if it.value >= 0.0 {
            write!(
                &mut o,
                r#"<rect x="{cx:.1}" y="{y}" width="{w:.1}" height="{bh}" fill="{fill}" opacity="0.82" rx="2"/>"#,
                w = w.max(1.5)
            )
            .and_then(|_| {
                write!(
                    &mut o,
                    r#"<text x="{tx:.1}" y="{ty}" fill="{fill}" font-size="11" font-weight="600">{t}</text>"#,
                    tx = cx + w + 6.0,
                    ty = y + bh - 2,
                    t = esc(&it.bar_text)
                )
            })
        } else {
            write!(
                &mut o,
                r#"<rect x="{x:.1}" y="{y}" width="{w:.1}" height="{bh}" fill="{fill}" opacity="0.82" rx="2"/>"#,
                x = cx - w,
                w = w.max(1.5)
            )
            .and_then(|_| {
                write!(
                    &mut o,
                    r#"<text x="{tx:.1}" y="{ty}" fill="{fill}" font-size="11" font-weight="600" text-anchor="end">{t}</text>"#,
                    tx = cx - w - 6.0,
                    ty = y + bh - 2,
                    t = esc(&it.bar_text)
                )
            })
        };

        if !it.note.is_empty() {
            let _ = write!(
                o,
                r#"<text x="{tx}" y="{ty}" fill="{DIM}" font-size="10" text-anchor="end">{n}</text>"#,
                tx = width - 6,
                ty = y + bh - 2,
                n = esc(&it.note)
            );
        }
    }

    o.push_str("</svg>");
    o
}

// ── 报告主体 ──────────────────────────────────────────────────────

pub fn render(r: &SentimentDaily) -> String {
    let mut h = String::with_capacity(180_000);

    let week = weekday_cn(&r.date);
    let gen = generated_str(r.generated_at);

    // ── head ──
    let _ = write!(
        h,
        r##"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>A股市场情绪日报 · {date}</title>
<style>{css}</style>
</head>
<body>
<div class="wrap">
"##,
        date = esc(&r.date),
        css = CSS
    );

    // ── 头部指数条 ──
    let mut idx_html = String::new();
    for ix in &r.breadth.indices {
        let _ = write!(
            idx_html,
            r#"<div class="idx-item"><span class="idx-name">{name}</span><span class="idx-close {c}">{close:.2}</span><span class="{c} sm">{p}</span></div>"#,
            name = esc(&ix.name),
            close = ix.close,
            c = cls(ix.pct_change),
            p = pct(ix.pct_change)
        );
    }
    if idx_html.is_empty() {
        idx_html = r#"<span class="dim sm">指数数据不可得</span>"#.to_string();
    }

    let _ = write!(
        h,
        r#"
  <div class="hd">
    <div>
      <h1>A股市场情绪日报 · <em>龙虎榜 / 题材热点 / 行业轮动</em></h1>
      <div class="sub">交易日期 {date}（星期{week}）· 收盘后复盘 · 数据抓取于 {gen}</div>
    </div>
    <div class="idxbar">{idx}</div>
  </div>
"#,
        date = esc(&r.date),
        idx = idx_html
    );

    // ── 概览 KPI ──
    let top_theme = r.themes.themes.first();
    let top_board = r.boards.first();
    let bot_board = r.boards.last();

    let theme_label = top_theme
        .map(|t| esc(&t.tag))
        .unwrap_or_else(|| "—".to_string());
    let theme_sub = match top_theme {
        Some(t) => format!(
            "{} 只共振 · 领涨 {} {}",
            t.count,
            esc(t.leader.as_deref().unwrap_or("—")),
            {
                let c = cls(t.leader_pct);
                format!("<span class=\"{c}\">{:+.2}%</span>", t.leader_pct)
            }
        ),
        None => "当日无题材数据".to_string(),
    };

    let board_up = match top_board {
        Some(b) => format!(
            "{} <span class=\"{}\">{:+.2}%</span>",
            esc(&b.name),
            cls(b.change_pct),
            b.change_pct
        ),
        None => "—".to_string(),
    };
    let board_up_sub = match top_board {
        Some(b) => format!(
            "领涨 {} · 涨 {} / 跌 {}",
            esc(b.leader.as_deref().unwrap_or("—")),
            b.up_count,
            b.down_count
        ),
        None => String::new(),
    };
    let board_down = match bot_board {
        Some(b) => format!(
            "{} <span class=\"{}\">{:+.2}%</span>",
            esc(&b.name),
            cls(b.change_pct),
            b.change_pct
        ),
        None => "—".to_string(),
    };
    let board_down_sub = format!("全市场 {} 个行业参与统计", r.boards.len());

    let _ = write!(
        h,
        r#"
  <div class="kpis">
    <div class="kpi">
      <div class="k-l">龙虎榜上榜家数</div>
      <div class="k-v">{n}<span class="dim" style="font-size:12px;font-weight:400"> 家</span></div>
      <div class="k-s">{rec} 条上榜记录 · 净买入 {nin} 家 / 净卖出 {nout} 家</div>
    </div>
    <div class="kpi k2">
      <div class="k-l">龙虎榜净买入合计</div>
      <div class="k-v {c}">{net}</div>
      <div class="k-s">买入方 {bin} · 卖出方 {bout}</div>
    </div>
    <div class="kpi k3">
      <div class="k-l">最热题材</div>
      <div class="k-v" style="font-size:16px">{theme}</div>
      <div class="k-s">{theme_sub}</div>
    </div>
    <div class="kpi k4">
      <div class="k-l">领涨 / 领跌行业</div>
      <div class="k-v" style="font-size:14px;line-height:1.45">{up}<br>{down}</div>
      <div class="k-s">{up_sub}<br>{board_down_sub}</div>
    </div>
    <div class="kpi k5">
      <div class="k-l">全市场涨跌家数</div>
      <div class="k-v"><span class="up">{u}</span> / <span class="down">{d}</span></div>
      <div class="k-s">覆盖 {cov} 只 · 平盘 {flat} · 成交额 {amt}</div>
    </div>
  </div>
"#,
        n = r.lhb.stock_count,
        rec = r.lhb.record_count,
        nin = r.lhb.net_buy_count,
        nout = r.lhb.net_sell_count,
        c = cls(r.lhb.net_total),
        net = money(r.lhb.net_total),
        bin = money(r.lhb.buy_total),
        bout = money_with_sign(r.lhb.sell_total, true),
        theme = theme_label,
        theme_sub = theme_sub,
        up = board_up,
        down = board_down,
        up_sub = esc(&board_up_sub),
        board_down_sub = esc(&board_down_sub),
        u = r.breadth.up_count,
        d = r.breadth.down_count,
        cov = r.breadth.covered_count,
        flat = r.breadth.flat_count,
        amt = money_opt(r.breadth.total_amount),
    );

    // ── tabs ──
    let _ = write!(
        h,
        r#"
  <div class="tabs">
    <button class="tab active" data-t="p1">龙虎榜
      <span class="badge">{n} 家 / {rec} 条</span></button>
    <button class="tab" data-t="p2">题材热点
      <span class="badge">{hs} 只 / {ts} 题材</span></button>
    <button class="tab" data-t="p3">行业轮动
      <span class="badge">{nb} 个行业</span></button>
  </div>
"#,
        n = r.lhb.stock_count,
        rec = r.lhb.record_count,
        hs = r.themes.stock_count,
        ts = r.themes.theme_total,
        nb = r.boards.len()
    );

    h.push_str(&panel_lhb(r));
    h.push_str(&panel_themes(r));
    h.push_str(&panel_boards(r));

    // ── 页脚 ──
    let mut src_html = String::new();
    for (i, s) in r.sources.iter().enumerate() {
        let _ = write!(
            src_html,
            r#"<span class="sb">{i} {s}</span>"#,
            i = ["①", "②", "③", "④", "⑤", "⑥", "⑦", "⑧"]
                .get(i)
                .copied()
                .unwrap_or("·"),
            s = esc(s)
        );
    }

    let mut notes_html = String::new();
    for n in &r.methodology.notes {
        let _ = write!(notes_html, r#"<div>· {n}</div>"#, n = esc(n));
    }

    let amount_note = if r.breadth.total_amount.is_none() {
        "<div>· 成交额不可得：腾讯与新浪日线接口均不含成交额字段，\
         仅最近一个交易日可通过板块快照与指数成交额补齐，故此处不做估算。</div>"
    } else {
        ""
    };

    // 两条取数路径的方法学完全不同，说明文字必须跟着走，否则会出现
    // 「报告用的是官方涨跌幅，页脚却声称是等权聚合」这种自相矛盾。
    let board_method_note = if r.methodology.board_method.contains("官方") {
        "行业涨跌幅为<b>东财板块官方涨跌幅</b>，成交额与涨跌家数同样取自官方板块快照；\
         市场宽度来自东财全市场快表，该快表仅覆盖沪深两市、不含北交所，\
         与历史交易日的逐股聚合口径不同，两者不可直接横向比较。"
    } else {
        "行业涨跌幅为<b>成分股日线等权算术平均</b>，即该行业成分股当日涨跌幅的算术平均，\
         并非东财板块指数的官方涨跌幅——东财未提供按历史日期查询板块指数的公开接口。"
    };

    let _ = write!(
        h,
        r#"
  <div class="ft">
    <div><b>数据来源</b>（全部为公开接口真实抓取，未做任何估算或补值）</div>
    <div class="src">{src}</div>
    <div><b>交易日期</b>：{date}（星期{week}）收盘数据</div>
    <div class="warn">
      <b>方法学说明</b>：{board_method_note}
      行业分类为东财二级行业口径，与东财当日行业板块清单名称对齐；
      「成分(东财)」列为东财官方披露的板块成分股家数。
      全市场涨跌家数覆盖 {cov} 只个股，停牌及当日无成交个股未计入。
      {amount_note}
      {notes_html}
    </div>
    <div style="margin-top:8px">生成时间 {gen} ·
      {elapsed} · 单文件 HTML，无外部依赖，可离线打开</div>
  </div>

</div>
<script>{script}</script>
</body>
</html>
"#,
        src = src_html,
        date = esc(&r.date),
        board_method_note = board_method_note,
        cov = r.breadth.covered_count,
        gen = gen,
        elapsed = if r.methodology.from_cache {
            "读取自缓存".to_string()
        } else {
            format!("计算耗时 {} ms", r.methodology.elapsed_ms)
        },
        script = SCRIPT
    );

    h
}

// ── 面板一：龙虎榜 ────────────────────────────────────────────────

/// 龙虎榜条目 → 双向条形图条目
///
/// 写成独立函数而非闭包：闭包无法表达「返回借用自参数的 `Div<'_>`」这一
/// 高阶生命周期约束，会触发 lifetime may not live long enough。
fn lhb_div(e: &appcore::DragonTigerEntry) -> Div<'_> {
    Div {
        label: e.name.as_str(),
        value: e.net_amount,
        bar_text: money(e.net_amount),
        note: pct(e.pct_change),
    }
}

fn panel_lhb(r: &SentimentDaily) -> String {
    let entries = &r.lhb.entries;

    // 净买入 TOP 15
    let top: Vec<Div> = entries.iter().take(15).map(lhb_div).collect();

    // 净卖出后 8 名
    let bot_start = entries.len().saturating_sub(8);
    let bot: Vec<Div> = entries[bot_start..].iter().map(lhb_div).collect();

    // 上榜原因 TOP 8
    let mut reasons = String::new();
    for rs in &r.lhb.top_reasons {
        let _ = write!(
            reasons,
            r#"<div class="rr"><span class="rr-c">{c}</span><span class="dim">{reason}</span></div>"#,
            c = rs.count,
            reason = esc(&rs.reason)
        );
    }
    if reasons.is_empty() {
        reasons = r#"<div class="dim">无上榜原因数据</div>"#.to_string();
    }

    // 明细表：标记同一只股票的多条记录
    let mut seen: HashMap<&str, bool> = HashMap::new();
    let mut rows = String::new();
    for (i, e) in entries.iter().enumerate() {
        let dup = seen.insert(e.code.as_str(), true).map_or(false, |_| true);
        let _ = write!(
            rows,
            r#"<tr>
<td class="idx">{i}</td><td class="code">{code}</td><td class="nm">{name}{dup}</td>
<td class="{c}">{p}</td><td class="{cn}">{net}</td><td>{buy}</td><td>{sell}</td>
<td>{turn:.2}%</td><td>{amt}</td><td class="rs">{reason}</td>
</tr>"#,
            i = i + 1,
            code = esc(&e.code),
            name = esc(&e.name),
            dup = if dup {
                r#"<sup class="dup">重</sup>"#
            } else {
                ""
            },
            c = cls(e.pct_change),
            p = pct(e.pct_change),
            cn = cls(e.net_amount),
            net = money(e.net_amount),
            buy = money(e.total_buy),
            sell = money_with_sign(e.total_sell, true),
            turn = e.turnover,
            amt = money(e.amount),
            reason = esc(&e.reason)
        );
    }

    let warn = if r.lhb.record_count > r.lhb.stock_count {
        format!(
            r#"<div class="warn"><b>口径说明</b>：净买入合计 <b>{net}</b> 为按 <b>{n} 只个股去重</b>后的结果（同一只股票触发多个上榜原因时只计一次，取该股最大净买入额）；{rec} 条记录直接求和为 <b>{sum}</b>。明细表按东财口径列出全部 {rec} 条记录，同一只股票重复出现时以 <sup class="dup">重</sup> 标记。</div>"#,
            net = money(r.lhb.net_total),
            n = r.lhb.stock_count,
            rec = r.lhb.record_count,
            sum = money(r.lhb.records_sum)
        )
    } else {
        String::new()
    };

    let empty = if entries.is_empty() {
        r#"<div class="dim">当日无龙虎榜数据（可能为非交易日）</div>"#
    } else {
        ""
    };

    format!(
        r#"
  <div class="panel active" id="p1">
    <div class="cols">
      <div class="sec">
        <div class="sec-h">
          <h2>龙虎榜净买入 TOP 15</h2>
          <span class="note">单位 ¥ · 右侧为当日涨跌幅</span>
        </div>
        {top_svg}
        <div class="sec-h" style="margin-top:14px">
          <h2>净卖出居前（后 8 名）</h2>
        </div>
        {bot_svg}
        <div class="legend">
          <span><i style="background:{UP}"></i>净买入（红）</span>
          <span><i style="background:{DOWN}"></i>净卖出（绿）</span>
        </div>
      </div>
      <div class="sec">
        <div class="sec-h"><h2>上榜原因 TOP 8</h2></div>
        {reasons}
        {warn}
      </div>
    </div>

    <div class="sec">
      <div class="sec-h">
        <h2>全市场龙虎榜明细（{rec} 条，按净买入降序）</h2>
        <span class="note">东方财富数据中心 · 上榜个股合计成交额 {amt}</span>
      </div>
      {empty}
      <div class="tw">
        <table>
          <thead><tr>
            <th>#</th><th>代码</th><th class="l">名称</th>
            <th>涨跌幅</th><th>净买入</th><th>买入额</th><th>卖出额</th>
            <th>换手率</th><th>成交额</th><th class="l">上榜原因</th>
          </tr></thead>
          <tbody>{rows}</tbody>
        </table>
      </div>
    </div>
  </div>
"#,
        top_svg = svg_diverging(&top, 1060, 25, 150, 96),
        bot_svg = svg_diverging(&bot, 1060, 25, 150, 96),
        rec = r.lhb.record_count,
        amt = money(r.lhb.amount_total),
    )
}

// ── 面板二：题材热点 ──────────────────────────────────────────────

fn panel_themes(r: &SentimentDaily) -> String {
    let themes = &r.themes.themes;
    let chart_n = themes.len().min(20);
    let chart = svg_theme_bars(&themes[..chart_n], 1060, 25, 170, 118);

    // 最热题材 TOP 8 成分股
    let mut top_members = String::new();
    for t in themes.iter().take(8) {
        let mut chips = String::new();
        for s in t.stocks.iter().take(6) {
            let _ = write!(
                chips,
                r#"<span class="mchip"><b>{n}</b><i class="{c}">{p}</i></span>"#,
                n = esc(&s.name),
                c = cls(s.pct_change),
                p = pct(s.pct_change)
            );
        }
        if t.stocks.len() > 6 {
            let _ = write!(
                chips,
                r#"<span class="mchip more">+{}</span>"#,
                t.stocks.len() - 6
            );
        }
        let _ = write!(
            top_members,
            r#"<div class="rr"><span><span class="rr-c">{c} 只</span> {tag}</span></div><div class="tmem" style="margin:4px 0 8px">{chips}</div>"#,
            c = t.count,
            tag = esc(&t.tag),
            chips = chips
        );
    }
    if top_members.is_empty() {
        top_members = r#"<div class="dim">当日无题材数据</div>"#.to_string();
    }

    // 题材卡片 TOP 36
    let mut cards = String::new();
    for t in themes.iter().take(36) {
        let mut mem = String::new();
        for s in t.stocks.iter().take(8) {
            let _ = write!(
                mem,
                r#"<span class="mchip"><b>{n}</b><i class="{c}">{p}</i></span>"#,
                n = esc(&s.name),
                c = cls(s.pct_change),
                p = pct(s.pct_change)
            );
        }
        if t.stocks.len() > 8 {
            let _ = write!(mem, r#"<span class="mchip more">+{}</span>"#, t.stocks.len() - 8);
        }
        let _ = write!(
            cards,
            r#"<div class="tcard">
<div class="tcard-h"><span class="tname">{tag}</span><span class="tcount">{c} 只</span></div>
<div class="tmeta">领涨 {leader} <span class="{lc}">{lp}</span> · 平均 <span class="{ac}">{avg}</span></div>
<div class="tmem">{mem}</div>
</div>"#,
            tag = esc(&t.tag),
            c = t.count,
            leader = esc(t.leader.as_deref().unwrap_or("—")),
            lc = cls(t.leader_pct),
            lp = pct(t.leader_pct),
            ac = cls(t.avg_pct),
            avg = pct(t.avg_pct),
            mem = mem
        );
    }

    // 强势股明细：从各题材成员中去重，按涨幅降序
    let mut all: Vec<ThemeStock> = Vec::new();
    for t in themes {
        for s in &t.stocks {
            if !all.iter().any(|x| x.code == s.code) {
                all.push(s.clone());
            }
        }
    }
    all.sort_by(|a, b| {
        b.pct_change
            .partial_cmp(&a.pct_change)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut rows = String::new();
    for (i, s) in all.iter().enumerate() {
        let tags: String = s
            .reason
            .split('+')
            .filter(|t| !t.trim().is_empty())
            .map(|t| format!(r#"<span class="tag">{t}</span>"#, t = esc(t.trim())))
            .collect();
        let _ = write!(
            rows,
            r#"<tr>
<td class="idx">{i}</td><td class="code">{code}</td><td class="nm">{name}</td>
<td class="{c}">{p}</td><td class="tags">{tags}</td>
</tr>"#,
            i = i + 1,
            code = esc(&s.code),
            name = esc(&s.name),
            c = cls(s.pct_change),
            p = pct(s.pct_change),
            tags = tags
        );
    }

    format!(
        r#"
  <div class="panel" id="p2">
    <div class="cols">
      <div class="sec">
        <div class="sec-h">
          <h2>题材热度 TOP {chart_n}（同花顺 reason 标签词频）</h2>
          <span class="note">条形长度 = 出现只数 · 颜色 = 该题材成分股平均涨跌幅</span>
        </div>
        {chart}
        <div class="legend">
          <span><i style="background:{UP}"></i>平均上涨</span>
          <span><i style="background:{DOWN}"></i>平均下跌</span>
          <span>口径：reason 字段按「+」拆分后词频统计，同一只股票的同一标签只计一次</span>
        </div>
      </div>
      <div class="sec">
        <div class="sec-h"><h2>最热题材 TOP 8</h2></div>
        {top_members}
        <div class="warn">
          当日 <b>{hs}</b> 只强势股共拆出 <b>{tt}</b> 个题材标签（去重后 <b>{ts}</b> 个）。
          题材标签来自同花顺编辑部人工运营归因，非交易所官方分类。
        </div>
      </div>
    </div>

    <div class="sec">
      <div class="sec-h">
        <h2>热门题材归因卡片（TOP 36）</h2>
        <span class="note">含成分股与当日涨幅</span>
      </div>
      <div class="tgrid">{cards}</div>
    </div>

    <div class="sec">
      <div class="sec-h">
        <h2>当日强势股明细（{hs} 只，按涨幅降序）</h2>
        <span class="note">同花顺 getharden</span>
      </div>
      <div class="tw">
        <table>
          <thead><tr>
            <th>#</th><th>代码</th><th class="l">名称</th>
            <th>涨幅</th><th class="l">题材归因</th>
          </tr></thead>
          <tbody>{rows}</tbody>
        </table>
      </div>
    </div>
  </div>
"#,
        hs = r.themes.stock_count,
        tt = r.themes.tag_total,
        ts = r.themes.theme_total,
    )
}

// ── 面板三：行业轮动 ──────────────────────────────────────────────

/// 行业板块 → 双向条形图条目（同样需为函数而非闭包，原因见 `lhb_div`）
fn board_div(b: &IndustryBoard) -> Div<'_> {
    Div {
        label: b.name.as_str(),
        value: b.change_pct,
        bar_text: pct(b.change_pct),
        note: format!("{}涨/{}跌", b.up_count, b.down_count),
    }
}

fn panel_boards(r: &SentimentDaily) -> String {
    let boards = &r.boards;
    let take = boards.len().min(12);

    // 与 render() 里的 board_method_note 同源判定。快路径下涨跌幅、涨跌家数、
    // 领涨股全部由板块快照直接给出，若沿用慢路径的「逐只统计」措辞，会和
    // 上方的方法学说明自相矛盾。
    let official = r.methodology.board_method.contains("官方");
    let legend_note = if official {
        "涨跌幅 / 涨跌家数 / 领涨股均为东财板块官方口径"
    } else {
        "涨跌幅为成分股等权算术平均"
    };
    let table_note = if official {
        "涨跌家数 / 领涨股 / 参与计算家数均取自东财板块官方快照"
    } else {
        "涨跌家数 / 领涨股 / 参与计算家数均由成分股逐只真实日线统计"
    };

    let top: Vec<Div> = boards[..take].iter().map(board_div).collect();
    let bot: Vec<Div> = if boards.len() > take {
        boards[boards.len() - take..].iter().map(board_div).collect()
    } else {
        Vec::new()
    };

    let mut rows = String::new();
    for (i, b) in boards.iter().enumerate() {
        let _ = write!(
            rows,
            r#"<tr>
<td class="idx">{i}</td><td class="nm2">{name}</td>
<td class="{c}">{p}</td><td class="up">{u}</td><td class="down">{d}</td><td class="flat">{f}</td>
<td>{mem}</td><td>{calc}</td><td>{amt}</td>
<td class="nm2">{leader}</td><td class="{lc}">{lp}</td>
</tr>"#,
            i = i + 1,
            name = esc(&b.name),
            c = cls(b.change_pct),
            p = pct(b.change_pct),
            u = b.up_count,
            d = b.down_count,
            f = b.flat_count,
            mem = if b.member_count > 0 {
                b.member_count.to_string()
            } else {
                "—".to_string()
            },
            calc = b.calc_count,
            amt = money_opt(b.amount),
            leader = esc(b.leader.as_deref().unwrap_or("—")),
            lc = cls(b.leader_pct),
            lp = pct(b.leader_pct)
        );
    }

    let bot_svg = if bot.is_empty() {
        String::new()
    } else {
        format!(
            r#"<div class="sec-h" style="margin-top:14px"><h2>行业涨跌幅榜 · 领跌 TOP {n}</h2></div>{svg}"#,
            n = bot.len(),
            svg = svg_diverging(&bot, 1060, 25, 150, 96)
        )
    };

    format!(
        r#"
  <div class="panel" id="p3">
    <div class="sec">
      <div class="sec-h">
        <h2>行业涨跌幅榜 · 领涨 TOP {n}</h2>
        <span class="note">红涨绿跌 · 右侧为板块涨跌幅</span>
      </div>
      {top_svg}
      {bot_svg}
      <div class="legend">
        <span><i style="background:{UP}"></i>上涨</span>
        <span><i style="background:{DOWN}"></i>下跌</span>
        <span>{legend_note}</span>
      </div>
    </div>

    <div class="sec">
      <div class="sec-h">
        <h2>行业板块全景（{nb} 个行业，按涨跌幅降序）</h2>
        <span class="note">{table_note}</span>
      </div>
      <div class="tw">
        <table>
          <thead><tr>
            <th>#</th><th class="l">行业板块</th><th>涨跌幅</th>
            <th>上涨</th><th>下跌</th><th>平盘</th>
            <th>成分(东财)</th><th>计算家数</th><th>成交额</th>
            <th class="l">领涨股</th><th>领涨股涨幅</th>
          </tr></thead>
          <tbody>{rows}</tbody>
        </table>
      </div>
    </div>
  </div>
"#,
        n = top.len(),
        top_svg = svg_diverging(&top, 1060, 25, 150, 96),
        nb = boards.len(),
    )
}
