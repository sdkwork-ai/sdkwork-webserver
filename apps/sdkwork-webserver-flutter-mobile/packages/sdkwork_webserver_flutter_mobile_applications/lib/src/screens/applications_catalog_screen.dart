/// Route-level capability screen.
///
/// The root shell mounts this; business UI lives in capability packages
/// (`FLUTTER_APP_MOBILE_ARCHITECTURE_SPEC.md` section 4). The screen renders the
/// view model it is handed and owns no I/O.
library;

import 'package:flutter/material.dart';
import 'package:sdkwork_webserver_flutter_mobile_commons/sdkwork_webserver_flutter_mobile_commons.dart';

import '../models/application_models.dart';

class WebserverFlutterApplicationsCatalogScreen extends StatelessWidget {
  const WebserverFlutterApplicationsCatalogScreen({
    super.key,
    required this.model,
    this.onRefresh,
    this.onLoadMore,
  });

  final WebserverFlutterApplicationsScreenModel model;
  final Future<void> Function()? onRefresh;
  final Future<void> Function()? onLoadMore;

  @override
  Widget build(BuildContext context) {
    final status = resolveWebserverFlutterScreenStatus(
      model.rows.length,
      model.loading,
      model.errorMessage,
    );
    if (status != WebserverFlutterScreenStatus.ready) {
      final messageKey = resolveWebserverFlutterScreenMessageKey(status);
      return Scaffold(
        appBar: AppBar(title: const Text('SDKWork Web Server')),
        body: WebserverFlutterStatusView(
          message: messageKey.isEmpty
              ? model.emptyMessage
              : _resolveStatusMessage(status),
        ),
      );
    }
    final list = ListView.separated(
      itemCount: model.rows.length,
      separatorBuilder: (context, index) => const Divider(height: 1),
      itemBuilder: (context, index) => _buildRow(model.rows[index]),
    );
    return Scaffold(
      appBar: AppBar(title: const Text('SDKWork Web Server')),
      body: model.hasMore
          ? Column(
              children: <Widget>[
                Expanded(child: list),
                if (model.appending)
                  const Padding(
                    padding: EdgeInsets.all(WebserverFlutterTokens.spacingMd),
                    child: CircularProgressIndicator(),
                  )
                else
                  TextButton(
                    onPressed: onLoadMore == null ? null : () => onLoadMore!(),
                    child: Text(model.loadMoreLabel),
                  ),
              ],
            )
          : list,
    );
  }

  String _resolveStatusMessage(WebserverFlutterScreenStatus status) {
    switch (status) {
      case WebserverFlutterScreenStatus.loading:
        return model.loadingMessage;
      case WebserverFlutterScreenStatus.empty:
        return model.emptyMessage;
      case WebserverFlutterScreenStatus.error:
        return model.errorMessage;
      case WebserverFlutterScreenStatus.ready:
        return '';
    }
  }

  Widget _buildRow(WebserverFlutterApplicationRow row) {
    return ListTile(
      title: Text(row.name),
      subtitle: Text(
        row.item.description.isEmpty
            ? '${row.kindLabel} · ${row.statusLabel}'
            : '${row.kindLabel} · ${row.statusLabel} · ${row.item.description}',
      ),
      trailing: Text('${row.item.platformTargetCount}'),
    );
  }
}
