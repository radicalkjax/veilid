import { veilidClient, VeilidRoutingContext, RecordKey } from 'veilid-wasm';

export const waitForMs = (milliseconds: number) => {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
};

export const asyncCallWithTimeout = async<T>(asyncPromise: Promise<T>, timeLimit: number): Promise<T> => {
  let timeoutHandle: ReturnType<typeof setTimeout>;

  const timeoutPromise = new Promise<T>((_resolve, reject) => {
    timeoutHandle = setTimeout(
      () => reject(new Error('Async call timeout limit reached')),
      timeLimit
    );
  });

  return Promise.race([asyncPromise, timeoutPromise]).then(result => {
    clearTimeout(timeoutHandle);
    return result;
  })
}

export const waitForPublicAttachment = async () => {
  //console.log('waitForPublicAttachment...');

  const startTime = Date.now();
  let looped = false;
  while (true) {

    const state = await veilidClient.getState();
    if (state.attachment.publicInternetReady) {
      let attached = false
      switch (state.attachment.state) {
        case "Detached":
        case "Detaching":
        case "Attaching":
          break;
        default:
          attached = true;
          break;
      }
      if (attached) {
        break;
      }
    }
    await waitForMs(250);
    //console.log('waitForPublicAttachment loop...');
    looped = true;
  }
  if (looped) {
    console.log(`waitForPublicAttachment took ${Date.now() - startTime}ms`);
  }
}

export const waitForDetached = async () => {
  while (true) {
    const state = await veilidClient.getState();
    let detached = false
    switch (state.attachment.state) {
      case "Detached":
        detached = true;
        break;
      default:
        break;
    }
    if (detached) {
      break;
    }
    await waitForMs(250);
  }
}

export const waitForShutdown = async () => {
  while (true) {
    const isShutdown = veilidClient.isShutdown();
    if (isShutdown) {
      break;
    }
    await waitForMs(250);
  }
}

export const waitForOfflineSubkeyWrite = async (routingContext: VeilidRoutingContext, key: RecordKey) => {
  while ((await routingContext.inspectDHTRecord(key)).offlineSubkeys.length != 0) {
    await waitForMs(250);
  }
}

export const getLogTimestamp = () => {
  const now = new Date();

  // Get time components
  const hours = now.getHours();
  const minutes = now.getMinutes();
  const seconds = now.getSeconds();
  const milliseconds = now.getMilliseconds();

  // Pad with leading zeros where necessary to ensure two digits for hh, mm, ss
  const hh = hours.toString().padStart(2, '0');
  const mm = minutes.toString().padStart(2, '0');
  const ss = seconds.toString().padStart(2, '0');

  // Pad milliseconds with leading zeros to ensure three digits (000-999)
  const msec = milliseconds.toString().padStart(3, '0');

  return `${hh}:${mm}:${ss}.${msec}`;
}

export const waitForSingleEvent = <T extends Event>(element: EventTarget, eventName: string): Promise<T> => {

  let send: (value: T) => void;
  const promise = new Promise<T>((resolve) => {
    send = resolve;
  });

  // Define the listener function
  const listener = (event: Event) => {
    // Remove the listener once the event fires to prevent memory leaks
    element.removeEventListener(eventName, listener);
    // Resolve the promise with the event data
    send(event as T);
  };

  // Add the listener immediately
  element.addEventListener(eventName, listener);

  return promise;
}

// A fire-and-forget app message can be slow or silently dropped on a single-threaded WASM node
// whose public-internet readiness flaps and whose routes occasionally fail a hop. Resend until
// received, best-effort re-confirming readiness between attempts, bounded by attempts and a
// per-attempt timeout. One listener spans all attempts, so a late copy of an earlier send still
// satisfies it and a resend never double-counts a delivered result.
export const sendUntilReceived = async <T extends Event>(
  send: () => Promise<void>,
  eventTarget: EventTarget,
  eventName: string,
  perAttemptMs = 15_000,
  attempts = 3,
): Promise<T> => {
  const oneShot = waitForSingleEvent<T>(eventTarget, eventName);
  let lastError: unknown;
  for (let attempt = 1; attempt <= attempts; attempt++) {
    if (attempt > 1) {
      await asyncCallWithTimeout(waitForPublicAttachment(), perAttemptMs).catch(() => { });
    }
    await send();
    try {
      return await asyncCallWithTimeout<T>(oneShot, perAttemptMs);
    } catch (error) {
      lastError = error;
    }
  }
  throw lastError;
}