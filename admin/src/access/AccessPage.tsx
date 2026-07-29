import { Tabs } from '@heroui/react';
import { useSearchParams } from 'react-router-dom';

import { PageHeader } from '../ui/PageHeader';
import { ApiKeysPanel } from './ApiKeysPanel';
import { DevicesPanel } from './DevicesPanel';

type AccessTab = 'devices' | 'api-keys';

export function AccessPage() {
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
        description="Review signed-in devices and manage credentials used by external applications."
        title="Access"
      />

      <Tabs
        aria-label="Access management"
        onSelectionChange={selectTab}
        selectedKey={tab}
        variant="secondary"
      >
        <Tabs.List>
          <Tabs.Tab id="devices">
            Devices
            <Tabs.Indicator />
          </Tabs.Tab>
          <Tabs.Tab id="api-keys">
            API Keys
            <Tabs.Indicator />
          </Tabs.Tab>
        </Tabs.List>
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
