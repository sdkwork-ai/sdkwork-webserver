/// View models for the applications capability.
///
/// API DTOs come from the injected `deploy_app` catalog port; this file owns
/// view models and screen models only
/// (`FLUTTER_APP_MOBILE_ARCHITECTURE_SPEC.md` — local `models/` owns view models
/// and route params).
library;

class WebserverFlutterApplicationItem {
  const WebserverFlutterApplicationItem({
    required this.id,
    required this.name,
    required this.slug,
    required this.appKind,
    required this.appStatus,
    required this.description,
    required this.defaultEnvironment,
    required this.latestReleaseTag,
    required this.platformTargetCount,
    required this.updatedAt,
  });

  final String id;
  final String name;
  final String slug;
  final String appKind;
  final String appStatus;
  final String description;

  /// Empty string renders as "not set" instead of a placeholder glyph.
  final String defaultEnvironment;
  final String latestReleaseTag;
  final int platformTargetCount;
  final String updatedAt;
}

/// A list row: the view model plus the labels the list renders.
class WebserverFlutterApplicationRow {
  const WebserverFlutterApplicationRow({
    required this.item,
    required this.kindLabel,
    required this.statusLabel,
  });

  final WebserverFlutterApplicationItem item;
  final String kindLabel;
  final String statusLabel;

  String get id => item.id;
  String get name => item.name;
  String get description => item.description;
}

class WebserverFlutterApplicationsRouteParams {
  const WebserverFlutterApplicationsRouteParams({this.appId});

  final String? appId;
}

/// Everything the applications catalog screen renders.
class WebserverFlutterApplicationsScreenModel {
  const WebserverFlutterApplicationsScreenModel({
    required this.rows,
    required this.loading,
    required this.appending,
    required this.hasMore,
    required this.errorMessage,
    required this.errorDetail,
    required this.page,
    required this.totalItems,
    required this.emptyMessage,
    required this.loadingMessage,
    required this.loadingMoreMessage,
    required this.retryLabel,
    required this.loadMoreLabel,
  });

  final List<WebserverFlutterApplicationRow> rows;
  final bool loading;

  /// True while an *additional* page is in flight rather than the first one.
  final bool appending;

  final bool hasMore;

  /// Display copy for the last failure.
  final String errorMessage;

  /// Technical detail of the last failure; never treated as display copy.
  final String errorDetail;

  final int page;
  final int totalItems;
  final String emptyMessage;
  final String loadingMessage;
  final String loadingMoreMessage;
  final String retryLabel;
  final String loadMoreLabel;

  WebserverFlutterApplicationsScreenModel copyWith({
    List<WebserverFlutterApplicationRow>? rows,
    bool? loading,
    bool? appending,
    bool? hasMore,
    String? errorMessage,
    String? errorDetail,
    int? page,
    int? totalItems,
  }) {
    return WebserverFlutterApplicationsScreenModel(
      rows: rows ?? this.rows,
      loading: loading ?? this.loading,
      appending: appending ?? this.appending,
      hasMore: hasMore ?? this.hasMore,
      errorMessage: errorMessage ?? this.errorMessage,
      errorDetail: errorDetail ?? this.errorDetail,
      page: page ?? this.page,
      totalItems: totalItems ?? this.totalItems,
      emptyMessage: emptyMessage,
      loadingMessage: loadingMessage,
      loadingMoreMessage: loadingMoreMessage,
      retryLabel: retryLabel,
      loadMoreLabel: loadMoreLabel,
    );
  }
}
