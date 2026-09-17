/// Auth gate and first-route mount for the Flutter mobile root.
///
/// The gate decides access with the shell's pure decision function *before* any
/// page builds, then mounts the capability screen. Route guards are shell/runtime
/// responsibilities; capability packages only declare auth mode and permission
/// hints.
library;

import 'package:flutter/material.dart';
import 'package:sdkwork_webserver_flutter_mobile_applications/sdkwork_webserver_flutter_mobile_applications.dart';
import 'package:sdkwork_webserver_flutter_mobile_commons/sdkwork_webserver_flutter_mobile_commons.dart';
import 'package:sdkwork_webserver_flutter_mobile_shell/sdkwork_webserver_flutter_mobile_shell.dart';

import 'app.dart';
import 'bootstrap/runtime.dart';
import 'bootstrap/sdk_clients.dart';

class WebserverFlutterAuthGate extends StatefulWidget {
  const WebserverFlutterAuthGate({super.key});

  @override
  State<WebserverFlutterAuthGate> createState() =>
      _WebserverFlutterAuthGateState();
}

class _WebserverFlutterAuthGateState extends State<WebserverFlutterAuthGate> {
  /// One notifier for the widget's whole lifetime.
  ///
  /// Rebuilding it per frame would drop every notification, because the view
  /// model pushes into the instance it was handed at construction.
  final ValueNotifier<WebserverFlutterApplicationsScreenModel?> _state =
      ValueNotifier<WebserverFlutterApplicationsScreenModel?>(null);

  WebserverFlutterApplicationsCatalogViewModel? _viewModel;

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (_viewModel != null) {
      return;
    }
    final runtime = WebserverFlutterRuntimeScope.of(context);
    final clients = runtime.sdkClients;
    if (!runtime.ready || clients == null) {
      return;
    }
    final viewModel = WebserverFlutterApplicationsCatalogViewModel(
      service: createWebserverFlutterApplicationsService(clients),
      resolveMessage: (key) =>
          webserverFlutterApplicationsMessagesEnUs[key] ?? key,
      onStateChange: (state) => _state.value = state,
    );
    _viewModel = viewModel;
    // Not awaited: the view model owns its loading state and reports it through
    // onStateChange, so the first frame renders "loading" immediately.
    viewModel.refresh().ignore();
  }

  @override
  void dispose() {
    _state.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final runtime = WebserverFlutterRuntimeScope.of(context);

    if (!runtime.ready) {
      return _StatusScreen(
        title: 'SDKWork Web Server',
        message: runtime.failure ?? 'runtime configuration is unavailable',
      );
    }

    final applicationsRoute = runtime.routes.firstWhere(
      (route) => route.id == 'app.webserver.applications.list',
      orElse: () => webserverFlutterApplicationsRouteContributions.first,
    );
    final decision = resolveWebserverFlutterRouteAccess(
      route: applicationsRoute,
      isAuthenticated: false,
      hasPermission: runtime.iam.hasPermission,
    );
    if (!decision.allowed) {
      return _StatusScreen(
        title: 'SDKWork Web Server',
        message: 'access denied: ${decision.reason?.name ?? 'unknown'}',
      );
    }

    return ValueListenableBuilder<WebserverFlutterApplicationsScreenModel?>(
      valueListenable: _state,
      builder: (context, state, _) {
        if (state == null) {
          return const _StatusScreen(
            title: 'SDKWork Web Server',
            message: '',
          );
        }
        return WebserverFlutterApplicationsCatalogScreen(
          model: state,
          onRefresh: _viewModel?.refresh,
          onLoadMore: _viewModel?.loadMore,
        );
      },
    );
  }
}

class _StatusScreen extends StatelessWidget {
  const _StatusScreen({required this.title, required this.message});

  final String title;
  final String message;

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      appBar: AppBar(title: Text(title)),
      body: WebserverFlutterStatusView(message: message),
    );
  }
}
