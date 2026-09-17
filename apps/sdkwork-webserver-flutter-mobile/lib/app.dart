import 'package:flutter/material.dart';
import 'package:sdkwork_webserver_flutter_mobile_commons/sdkwork_webserver_flutter_mobile_commons.dart';

import 'auth_gate.dart';
import 'bootstrap/runtime.dart';

class WebserverFlutterApp extends StatelessWidget {
  const WebserverFlutterApp({required this.runtime, super.key});

  final WebserverFlutterRuntime runtime;

  @override
  Widget build(BuildContext context) {
    return WebserverFlutterRuntimeScope(
      runtime: runtime,
      child: MaterialApp(
        title: 'SDKWork Web Server',
        theme: ThemeData(colorSchemeSeed: Color(WebserverFlutterTokens.colorPrimary)),
        home: const WebserverFlutterAuthGate(),
      ),
    );
  }
}

/// Runtime scope every widget below reads instead of re-resolving the profile.
class WebserverFlutterRuntimeScope extends InheritedWidget {
  const WebserverFlutterRuntimeScope({
    required this.runtime,
    required super.child,
    super.key,
  });

  final WebserverFlutterRuntime runtime;

  static WebserverFlutterRuntime of(BuildContext context) {
    final scope = context
        .dependOnInheritedWidgetOfExactType<WebserverFlutterRuntimeScope>();
    if (scope == null) {
      throw StateError('WebserverFlutterRuntimeScope is not available.');
    }
    return scope.runtime;
  }

  @override
  bool updateShouldNotify(WebserverFlutterRuntimeScope oldWidget) {
    return !identical(runtime, oldWidget.runtime);
  }
}
