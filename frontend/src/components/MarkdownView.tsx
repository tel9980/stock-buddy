import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeRaw from 'rehype-raw';
import rehypeSanitize, { defaultSchema } from 'rehype-sanitize';

interface MarkdownViewProps {
  content: string;
  className?: string;
}

/**
 * 安全且美观的 Markdown 渲染组件
 *
 * - remark-gfm：表格、删除线、任务列表等 GFM 语法
 * - rehype-raw：渲染模型偶尔返回的 HTML（<h1>/<table>/<strong> 等）
 * - rehype-sanitize：默认 schema 会删除所有 style / className，导致 HTML 报告"像没解析"。
 *   这里在默认 schema 基础上放开 className（LLM 输出不可信，一律不放开 style ——
 *   否则可注入 position:fixed 覆盖层 / background:url() 外链探测等 CSS 注入）。
 * - 内置 .markdown-body 统一样式类，确保流式输出与历史详情渲染效果一致
 * - 预处理 markdown：修复模型返回的不规范标题（如 ###标题 → ### 标题）
 */
const displaySchema = {
  ...defaultSchema,
  attributes: {
    ...defaultSchema.attributes,
    '*': [
      ...(defaultSchema.attributes?.['*'] ?? []),
      'className',
    ],
    a: [...(defaultSchema.attributes?.a ?? []), 'target', 'rel'],
  },
};

/**
 * 修复模型返回的不规范 markdown
 * - 标题缺少空格：###标题 → ### 标题
 * - 行内 **加粗** 紧贴中文导致解析失败的情况由 remark-gfm 处理
 */
function normalizeMarkdown(src: string): string {
  // 修复标题：行首的 ##、###、#### 等后面紧跟非空格字符时补一个空格
  return src.replace(/^(#{1,6})([^\s#])/gm, '$1 $2');
}

export default function MarkdownView({ content, className }: MarkdownViewProps) {
  const cls = ['markdown-body', className].filter(Boolean).join(' ');
  const normalized = normalizeMarkdown(content);
  return (
    <div className={cls}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeRaw, [rehypeSanitize, displaySchema]]}
      >
        {normalized}
      </ReactMarkdown>
    </div>
  );
}
