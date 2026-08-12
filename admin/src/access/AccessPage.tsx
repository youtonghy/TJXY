import { Tabs } from '@heroui/react';
import { useSearchParams } from 'react-router-dom';

import { PageHeader } from '../ui/PageHeader';
import { useTranslate } from '../settings/i18n';
import { ApiKeysPanel } from './ApiKeysPanel';
import { DevicesPanel } from './DevicesPanel';

type AccessTab = 'devices' | 'api-keys';

export function AccessPage() {
  const tr = useTranslate();
  const [searchParams, setSearchParams] = useSearchParams();
  const tab = parseAccessTab(searchParams);

  const selectTab = (key: React.Key) => {
    if (key !== 'devices' && key !== 'api-keys') return;
    const next = new URLSearchParams(searchParams);
    if (key === 'devices') next.delete('tab');
    else next.set('tab', key);
    setSearchParams(next);
  };

  return (
    <div className="space-y-6">
      <PageHeader
        description={tr('Review signed-in devices and manage credentials used by external applications.', '查看已登录设备，并管理外部应用使用的凭据。')}
        title={tr('Access', '访问控制')}
      />

      <Tabs
        aria-label={tr('Access management', '访问控制管理')}
        onSelectionChange={selectTab}
        selectedKey={tab}
        variant="secondary"
      >
        <Tabs.ListContainer>
          <Tabs.List>
            <Tabs.Tab id="devices">
              {tr('Devices', '设备')}
              <Tabs.Indicator />
            </Tabs.Tab>
            <Tabs.Tab id="api-keys">
              {tr('API Keys', 'API 密钥')}
              <Tabs.Indicator />
            </Tabs.Tab>
          </Tabs.List>
        </Tabs.ListContainer>
        <Tabs.Panel className="pt-6" id={tab} key={tab}>
          {tab === 'devices' ? <DevicesPanel /> : <ApiKeysPanel />}
        </Tabs.Panel>
      </Tabs>
    </div>
  );
}

function parseAccessTab(searchParams: URLSearchParams): AccessTab {
  return searchParams.get('tab') === 'api-keys' ? 'api-keys' : 'devices';
}
