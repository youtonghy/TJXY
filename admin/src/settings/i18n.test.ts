import { translate } from './i18n';

it('loads administrator navigation labels from the Chinese locale file', () => {
  expect(translate('zh-CN', 'admin.navigation.manage')).toBe('管理');
  expect(translate('zh-CN', 'admin.navigation.systemSettings')).toBe('系统设置');
});

it('falls back to the English locale file when a key is missing in Chinese', () => {
  expect(translate('zh-CN', 'admin.system.save')).toBe('保存设置');
  expect(translate('en-US', 'admin.system.save')).toBe('Save settings');
});
