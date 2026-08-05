import { useState, useEffect, useCallback } from 'react';
import {
  List, Tabs, Drawer, Tag, Empty, Spin, Popconfirm, Button, message, ConfigProvider, theme,
} from 'antd';
import { api, AiHistoryItem, AiAnalysisHistory, AnalysisType } from '../api';
import MarkdownView from '../components/MarkdownView';

const TYPE_META: Record<AnalysisType, { label: string; color: string }> = {
  diagnose: { label: '个股诊断', color: '#e8804e' },
  predict: { label: '趋势预测', color: '#5b8ff9' },
  market: { label: '市场研判', color: '#9b6dff' },
  chat: { label: '智能问答', color: '#22c55e' },
};

type FilterKey = AnalysisType | 'all';

export default function AiHistory() {
  const [active, setActive] = useState<FilterKey>('all');
  const [items, setItems] = useState<AiHistoryItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [detail, setDetail] = useState<AiAnalysisHistory | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [drawerOpen, setDrawerOpen] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const data = await api.getAiHistory(active === 'all' ? undefined : (active as AnalysisType));
      setItems(data);
    } catch (e) {
      message.error(e instanceof Error ? e.message : '加载历史失败');
    } finally {
      setLoading(false);
    }
  }, [active]);

  useEffect(() => {
    load();
  }, [load]);

  const openDetail = async (id: number) => {
    setDrawerOpen(true);
    setDetailLoading(true);
    setDetail(null);
    try {
      const data = await api.getAiHistoryDetail(id);
      setDetail(data);
    } catch (e) {
      message.error(e instanceof Error ? e.message : '加载详情失败');
    } finally {
      setDetailLoading(false);
    }
  };

  const handleDelete = async (id: number) => {
    try {
      await api.deleteAiHistory(id);
      message.success('已删除');
      setItems(prev => prev.filter(i => i.id !== id));
      if (detail?.id === id) setDrawerOpen(false);
    } catch (e) {
      message.error(e instanceof Error ? e.message : '删除失败');
    }
  };

  const tabItems = [
    { key: 'all', label: '全部' },
    { key: 'diagnose', label: '个股诊断' },
    { key: 'predict', label: '趋势预测' },
    { key: 'market', label: '市场研判' },
    { key: 'chat', label: '智能问答' },
  ];

  return (
    <ConfigProvider theme={{ algorithm: theme.darkAlgorithm }}>
      <div style={{ display: 'flex', flexDirection: 'column', height: '100%', gap: '16px' }}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <h2 style={{ margin: 0 }}>AI 分析历史</h2>
          <Button onClick={load}>刷新</Button>
        </div>

        <Tabs
          activeKey={active}
          items={tabItems}
          onChange={(key) => setActive(key as FilterKey)}
        />

        <div className="ai-report-container" style={{ padding: '8px 16px' }}>
          <List
            loading={loading}
            dataSource={items}
            locale={{ emptyText: <Empty description="暂无分析记录，去 AI 诊断 / 研判 / 问答生成吧" /> }}
            renderItem={(item: AiHistoryItem) => (
              <List.Item
                actions={[
                  <Button key="view" type="link" onClick={() => openDetail(item.id)}>查看</Button>,
                  <Popconfirm
                    key="del"
                    title="确认删除这条记录？"
                    onConfirm={() => handleDelete(item.id)}
                    okText="删除"
                    cancelText="取消"
                  >
                    <Button type="link" danger>删除</Button>
                  </Popconfirm>,
                ]}
              >
                <List.Item.Meta
                  avatar={
                    <Tag color={TYPE_META[item.analysis_type].color}>
                      {TYPE_META[item.analysis_type].label}
                    </Tag>
                  }
                  title={item.title}
                  description={
                    <div>
                      <div style={{ color: 'var(--text-dim)', fontSize: '12px', marginBottom: '4px' }}>
                        {item.created_at}
                        {item.model ? ` · ${item.model}` : ''}
                        {item.stock_code ? ` · ${item.stock_code}` : ''}
                      </div>
                      <div style={{ color: 'var(--text-dim)', lineHeight: 1.6 }}>{item.summary}</div>
                    </div>
                  }
                />
              </List.Item>
            )}
          />
        </div>

        <Drawer
          title={detail?.title ?? '分析详情'}
          placement="right"
          width={760}
          open={drawerOpen}
          onClose={() => setDrawerOpen(false)}
          styles={{
            body: { background: 'var(--bg-surface)', color: 'var(--text)' },
            header: { background: 'var(--bg-surface)', color: 'var(--text)', borderBottom: '1px solid var(--border)' },
            mask: { background: 'rgba(0,0,0,0.6)' },
          }}
        >
          {detailLoading ? (
            <div style={{ display: 'flex', justifyContent: 'center', padding: '40px' }}>
              <Spin />
            </div>
          ) : detail ? (
            <div>
              <div style={{ marginBottom: '12px', color: 'var(--text-dim)', fontSize: '12px' }}>
                {detail.created_at}
                {' · '}
                {TYPE_META[detail.analysis_type].label}
                {detail.model ? ` · ${detail.model}` : ''}
              </div>
              <div className="ai-report-content">
                <MarkdownView content={detail.content} />
              </div>
            </div>
          ) : null}
        </Drawer>
      </div>
    </ConfigProvider>
  );
}
