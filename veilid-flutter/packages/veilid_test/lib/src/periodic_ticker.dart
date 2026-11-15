// A platform periodic ticker. Native uses `dart:async` `Timer.periodic`; web
// uses `window.setInterval` (see the web impl for why).
export 'periodic_ticker_native.dart'
    if (dart.library.js_interop) 'periodic_ticker_web.dart';
