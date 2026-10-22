use crate::ai::llm::ChatMessage;

/// 输出格式硬性要求（插入到所有 prompt 中，避免模型返回 HTML 标签导致前端无法渲染）
const OUTPUT_FORMAT: &str = r#"

【输出格式要求 —— 极其重要】
- 严格使用 Markdown 语法输出，绝对禁止输出任何 HTML 标签（包括 h1 h2 strong table div br p 等）。
- 标题用 # 或 ## 或 ### 表示，加粗用 **文字**，列表用 - 或 1. 表示。
- 表格必须用 Markdown 表格语法：
| 列1 | 列2 |
|-----|-----|
| 值1 | 值2 |
- 不要使用 HTML 实体或转义，直接输出中文和正常符号。"#;

/// 系统角色设定
const SYSTEM_ROLE: &str = r#"你是一位资深的A股投资分析师，拥有CFA和CIIA资质，擅长技术分析、基本面分析和市场情绪研判。

你的分析原则：
1. 数据驱动：所有结论必须基于提供的数据，不编造数据
2. 多维交叉验证：技术面、基本面、资金面、市场情绪交叉印证
3. 风险优先：先评估风险，再讨论机会
4. 客观中立：不预设立场，根据数据得出结论
5. 量化思维：尽量给出具体的数值区间和概率判断

注意：
- 中国A股涨跌颜色：红色代表上涨，绿色代表下跌
- 所有价格单位为人民币元
- 涨跌幅单位为百分比
- 请用中文输出分析报告"#;

/// 构建个股诊断 Prompt
pub fn diagnosis_prompt(
    stock_name: &str,
    stock_code: &str,
    quote_json: &str,
    kline_summary: &str,
    indicators_json: &str,
    support_resistance_json: &str,
    market_json: &str,
) -> Vec<ChatMessage> {
    let user_content = format!(
        r#"请对以下个股进行全面诊断分析。

## 股票信息
- 名称：{stock_name}（{stock_code}）

## 实时行情
{quote_json}

## 近期K线摘要（近60个交易日）
{kline_summary}

## 技术指标
{indicators_json}

## 支撑位/压力位
{support_resistance_json}

## 市场环境
{market_json}

## 请输出以下内容（Markdown格式）：

### 一、综合评分
给出技术面、基本面、资金面、市场情绪四个维度的评分（0-100分），并计算综合得分。

### 二、趋势研判
分析当前处于什么趋势阶段（上升/震荡/下降），依据是什么。

### 三、关键价位分析
- 近期压力位及突破概率
- 近期支撑位及有效性
- 建议关注的买卖点位

### 四、技术指标解读
逐项解读 MACD、KDJ、RSI、布林带 的信号含义，并给出综合技术结论。

### 五、风险评估
列出当前主要风险点（至少3条），给出风险等级（低/中/高）。

### 六、操作建议
给出明确的操作建议（买入/持有/减仓/观望），建议仓位比例，止损位和目标位。

### 七、关注信号
列出后续需要重点关注的信号（至少3条）。{output_format}"#
        , output_format = OUTPUT_FORMAT
    );

    vec![
        ChatMessage::system(SYSTEM_ROLE),
        ChatMessage::user(user_content),
    ]
}

/// 构建市场研判 Prompt
pub fn market_prompt(
    market_json: &str,
    sectors_json: &str,
) -> Vec<ChatMessage> {
    let user_content = format!(
        r#"请对当前A股市场进行全面研判。

## 大盘总览
{market_json}

## 板块排行
{sectors_json}

## 请输出以下内容（Markdown格式）：

### 一、市场情绪定调
当前市场整体情绪如何（强多/弱多/中性/弱空/强空），依据是什么。

### 二、指数分析
逐一分析上证指数、深证成指、创业板指的走势特征和短期方向。

### 三、板块轮动
- 今日强势板块及驱动因素分析
- 资金流向特征
- 板块轮动方向预判

### 四、涨跌停分析
根据涨跌停家数判断市场赚钱效应和亏钱效应。

### 五、短期展望
给出未来1-3个交易日的市场预判，以及需要关注的关键事件。

### 六、操作策略
当前市场环境下的整体操作策略建议（进攻/防守/均衡），仓位建议。{output_format}"#
        , output_format = OUTPUT_FORMAT
    );

    vec![
        ChatMessage::system(SYSTEM_ROLE),
        ChatMessage::user(user_content),
    ]
}

/// 构建趋势预测 Prompt
pub fn prediction_prompt(
    stock_name: &str,
    stock_code: &str,
    quote_json: &str,
    kline_summary: &str,
    indicators_json: &str,
    multi_period_summary: &str,
) -> Vec<ChatMessage> {
    let user_content = format!(
        r#"请对以下个股进行趋势预测分析。

## 股票信息
- 名称：{stock_name}（{stock_code}）

## 实时行情
{quote_json}

## 日线K线摘要（近120日）
{kline_summary}

## 技术指标
{indicators_json}

## 多周期对比
{multi_period_summary}

## 请输出以下内容（Markdown格式）：

### 一、趋势判定
- 短期趋势（1-5个交易日）：方向 + 强度
- 中期趋势（1-4周）：方向 + 强度
- 长期趋势（1-3月）：方向 + 强度

### 二、关键价位
- 第一压力位 / 第二压力位（附突破概率）
- 第一支撑位 / 第二支撑位（附支撑强度）
- 近期需要关注的价格区间

### 三、量价分析
分析近期量价关系是否健康，有无异常放量/缩量信号。

### 四、形态识别
识别近期K线组合形态（如头肩底、双底、三角形等），并解读含义。

### 五、时间窗口
基于周期分析，给出可能发生变盘的时间窗口。

### 六、概率研判
- 未来5个交易日上涨概率：X%
- 预期波动区间：XX - XX 元
- 最大回撤风险：约 X%{output_format}"#
        , output_format = OUTPUT_FORMAT
    );

    vec![
        ChatMessage::system(SYSTEM_ROLE),
        ChatMessage::user(user_content),
    ]
}

/// 构建智能问答 Prompt
pub fn chat_prompt(
    question: &str,
    context_json: &str,
) -> Vec<ChatMessage> {
    let system = r#"你是一位专业的A股投资顾问，能够回答用户关于股票、市场、技术分析等方面的问题。

回答原则：
1. 基于提供的数据回答，不编造信息
2. 如果数据不足以回答，明确说明需要哪些额外信息
3. 回答要专业、准确、有深度
4. 使用Markdown格式输出，条理清晰
5. 涉及具体操作建议时，务必提示风险
6. 中国A股涨跌颜色：红色代表上涨，绿色代表下跌

输出格式要求（极其重要）：
- 严格使用 Markdown 语法输出，绝对禁止输出任何 HTML 标签（包括 h1 h2 strong table div br p 等）。
- 标题用 # 或 ## 或 ### 表示，加粗用 **文字**，表格必须用 Markdown 表格语法：
| 列1 | 列2 |
|-----|-----|
| 值1 | 值2 |
- 不要使用 HTML 实体或转义，直接输出中文和正常符号。"#;

    let user = format!(
        r#"## 用户问题
{question}

## 当前可用数据
{context_json}

请基于以上数据回答用户的问题。如果数据中有相关信息，请充分利用；如果数据不足以完整回答，请说明。{output_format}"#
        , output_format = OUTPUT_FORMAT
    );

    vec![
        ChatMessage::system(system),
        ChatMessage::user(user),
    ]
}
