import 'package:flutter/material.dart';

import 'app.dart';
import 'bootstrap/runtime.dart';
import 'bootstrap/sdkwork_tls_trust.dart';

Future<void> main() async {
  // The HTTPS trust layer must be installed before any HTTP client is
  // created, so it runs ahead of binding initialization and services.
  SdkworkTlsTrust.installForApp();

  WidgetsFlutterBinding.ensureInitialized();
  final runtime = bootstrap();
  runApp(WebserverFlutterApp(runtime: runtime));
}
