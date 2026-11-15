import { useEffect, useState } from 'react';
import { DhtStressTest } from './components/DhtStressTest';
import { describeVeilidError, startVeilid } from './veilid/veilid-core';

type Status = { state: 'starting' } | { state: 'running' } | { state: 'failed'; error: string };

function App() {
  const [status, setStatus] = useState<Status>({ state: 'starting' });

  useEffect(() => {
    startVeilid().then(
      () => setStatus({ state: 'running' }),
      (error: unknown) => setStatus({ state: 'failed', error: describeVeilidError(error) }),
    );
  }, []);

  return (
    <>
      <h1>DHT Stress Test</h1>
      {status.state === 'starting' && <p>Starting veilid and attaching...</p>}
      {status.state === 'failed' && <p>Failed to start: {status.error}</p>}
      {status.state === 'running' && <DhtStressTest />}
    </>
  );
}

export default App;
