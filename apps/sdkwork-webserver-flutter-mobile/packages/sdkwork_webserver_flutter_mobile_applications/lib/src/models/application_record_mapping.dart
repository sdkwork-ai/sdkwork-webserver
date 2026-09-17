/// Pure record -> view-model -> row mapping for the applications capability.
///
/// Free of I/O on purpose: the label tables and the message resolver arrive
/// **injected**, so the whole mapping is a deterministic function that a unit
/// test can exercise without a widget binding or a network double.
library;

import 'package:sdkwork_webserver_flutter_mobile_core/sdkwork_webserver_flutter_mobile_core.dart'
    show WebserverDeployAppRecord;

import '../copy/applications_messages.dart';
import 'application_models.dart';

/// Resolve a closed-set token onto its message key.
String lookupWebserverFlutterEnumMessageKey(
  List<WebserverFlutterEnumLabel> labels,
  String value,
  String fallbackKey,
) {
  for (final label in labels) {
    if (label.value == value) {
      return label.messageKey;
    }
  }
  return fallbackKey;
}

/// Map one catalog record onto the view model.
///
/// Returns `null` for a record the list cannot render (no identity), so a
/// malformed row is dropped instead of producing a blank card.
WebserverFlutterApplicationItem? mapWebserverFlutterApplicationRecord(
  WebserverDeployAppRecord? record,
) {
  if (record == null) {
    return null;
  }
  final id = record.id;
  if (id.isEmpty) {
    return null;
  }
  final name = record.name.isNotEmpty ? record.name : id;
  final count = record.platformTargetCount;
  return WebserverFlutterApplicationItem(
    id: id,
    name: name,
    slug: record.slug,
    appKind: record.appKind,
    appStatus: record.appStatus,
    description: record.description,
    defaultEnvironment: record.defaultEnvironment,
    latestReleaseTag: record.latestReleaseTag,
    platformTargetCount: count > 0 ? count : 0,
    updatedAt: record.updatedAt,
  );
}

/// Project a view model onto the row the list renders.
WebserverFlutterApplicationRow toWebserverFlutterApplicationRow(
  WebserverFlutterApplicationItem item,
  List<WebserverFlutterEnumLabel> kindLabels,
  List<WebserverFlutterEnumLabel> statusLabels,
  String Function(String key) resolveMessage,
  String unknownFallbackKey,
) {
  return WebserverFlutterApplicationRow(
    item: item,
    kindLabel: resolveMessage(
      lookupWebserverFlutterEnumMessageKey(
        kindLabels,
        item.appKind,
        unknownFallbackKey,
      ),
    ),
    statusLabel: resolveMessage(
      lookupWebserverFlutterEnumMessageKey(
        statusLabels,
        item.appStatus,
        unknownFallbackKey,
      ),
    ),
  );
}
