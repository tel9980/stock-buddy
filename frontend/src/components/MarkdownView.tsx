import ReactMarkdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import rehypeRaw from 'rehype-raw';
import rehypeSanitize, { defaultSchema } from 'rehype-sanitize';

interface MarkdownViewProps {
  content: string;
  className?: string;
}

/**
 * 安全且“好看”的 Markdown 渲染组件
 *
 * - remark-gfm：表格、删除线、任务列表等 GFM 语法
 * - rehype-raw：渲染模型偶尔返回的 HTML（<h1>/<table>/<strong> 等）
 * - rehype-sanitize：默认 schema 会**删除所有 style / className**，导致 HTML 报告“像没解析”。
 *   这里在默认 schema 基础上放开 style / className（并保留 a 的 target/rel），
 *   既能正常显示排版，又仍拦截 script / iframe / 事件处理器等危险内容。
 */
const displaySchema = {
  ...defaultSchema,
  attributes: {
    ...defaultSchema.attributes,
    '*': [
      ...(defaultSchema.attributes?.['*'] ?? []),
      'className',
      'style',
    ],
    a: [...(defaultSchema.attributes?.a ?? []), 'target', 'rel'],
  },
};

export default function MarkdownView({ content, className }: MarkdownViewProps) {
  return (
    <div className={className}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        rehypePlugins={[rehypeRaw, [rehypeSanitize, displaySchema]]}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}
