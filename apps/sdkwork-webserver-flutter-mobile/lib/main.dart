import 'package:flutter/material.dart';

import 'auth_gate.dart';
import 'bootstrap/runtime.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final runtime = bootstrap();
  runApp(WebserverFlutterApp(runtime: runtime));
}
