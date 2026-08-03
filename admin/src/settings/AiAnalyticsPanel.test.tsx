import { render, screen, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import type { AiAnalytics } from './aiSettingsApi';
import { AiAnalyticsPanel } from './AiAnalyticsPanel';

const analytics: AiAnalytics = {
  window: { today: '2026-08-03', startsAt: '2026-08-02T14:00:00Z', endsAt: '2026-08-03T14:00:00Z', timeZone: 'server-local +10:00' },
  summary: { totalRequests: 3, activeUsers: 2, successfulRequests: 2, failedRequests: 1, promptTokens: 100, completionTokens: 40, totalTokens: 140, knownTokenRequests: 3 },
  daily: [{ day: '2026-08-03', totalRequests: 3, successfulRequests: 2, failedRequests: 1, totalTokens: 140 }],
  users: [{ userId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11', username: 'admin', totalRequests: 2, successfulRequests: 2, totalTokens: 120, lastUsedAt: '2026-08-03T01:00:00Z' }],
  models: [{ modelId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12', displayName: '小白鲸', upstreamModelId: 'deepseek-v4-flash', totalRequests: 3, successfulRequests: 2, totalTokens: 140, lastUsedAt: '2026-08-03T01:00:00Z' }],
  recentFailures: [{ id: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f13', userId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f11', username: 'admin', modelId: '018f17ac-4e99-7ec5-b4fd-8f15ca9f4f12', modelDisplayName: '小白鲸', outcome: 'upstream_timeout', elapsedMs: 30000, startedAt: '2026-08-03T00:30:00Z' }],
};

it('renders summary, trend, user ranking, model ranking, and safe failures', () => {
  render(<AiAnalyticsPanel analytics={analytics} error={null} loading={false} onRetry={vi.fn()} />);
  expect(within(screen.getByLabelText('今日对话')).getByText('3 次')).toBeInTheDocument();
  expect(within(screen.getByLabelText('活跃用户')).getByText('2 位')).toBeInTheDocument();
  expect(screen.getByText('140')).toBeInTheDocument();
  expect(screen.getByRole('img', { name: '最近 14 天 AI 请求趋势' })).toBeInTheDocument();
  const userGrid = screen.getByRole('grid', { name: '用户对话排行' });
  expect(within(userGrid).getByText('admin')).toBeInTheDocument();
  expect(within(screen.getByRole('grid', { name: '模型用量排行' })).getByText('deepseek-v4-flash')).toBeInTheDocument();
  expect(within(screen.getByRole('grid', { name: 'AI 最近失败记录' })).getByText('上游超时')).toBeInTheDocument();
});

it('shows incomplete token coverage and supports retrying analytics independently', async () => {
  const onRetry = vi.fn();
  const { rerender } = render(<AiAnalyticsPanel analytics={{ ...analytics, summary: { ...analytics.summary, totalTokens: null, knownTokenRequests: 2 } }} error={null} loading={false} onRetry={onRetry} />);
  expect(screen.getByText('Token 数据不完整')).toBeInTheDocument();
  rerender(<AiAnalyticsPanel analytics={null} error={new Error('private upstream detail')} loading={false} onRetry={onRetry} />);
  expect(screen.queryByText('private upstream detail')).not.toBeInTheDocument();
  await userEvent.click(screen.getByRole('button', { name: '重试统计' }));
  expect(onRetry).toHaveBeenCalledOnce();
});
