// App messages and calls arrive on the update callback, which the harness owns; suites wait on
// these targets instead. EventTarget exists in every context we run in.
export const appMessageEventTarget = new EventTarget();
export const appCallEventTarget = new EventTarget();
