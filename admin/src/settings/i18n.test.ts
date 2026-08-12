import { translate } from './i18n';

it('loads administrator navigation labels from the Chinese locale file', () => {
  expect(translate('zh-CN', 'admin.navigation.manage')).toBe('管理');
  expect(translate('zh-CN', 'admin.navigation.systemSettings')).toBe('系统设置');
});

it('falls back to the English locale file when a key is missing in Chinese', () => {
  expect(translate('zh-CN', 'admin.system.save')).toBe('保存设置');
  expect(translate('en-US', 'admin.system.save')).toBe('Save settings');
});

it('uses inline Chinese translations for administrator pages that have not moved to keyed catalogs', () => {
  expect(translate('zh-CN', 'Announcements', '公告管理')).toBe('公告管理');
  expect(translate('zh-CN', 'Playback activity', '播放活动')).toBe('播放活动');
  expect(translate('zh-CN', 'AI assistant', 'AI 助手')).toBe('AI 助手');
  expect(translate('en-US', 'AI assistant', 'AI 助手')).toBe('AI assistant');
});

it('provides localized local metadata management labels', () => {
  expect(translate('zh-CN', 'admin.metadata.localTitle')).toBe('本地元数据存储');
  expect(translate('zh-CN', 'admin.metadata.localCleanup')).toBe('清理悬空元数据');
  expect(translate('en-US', 'admin.metadata.localRestart')).toBe('Server restart required');
});
