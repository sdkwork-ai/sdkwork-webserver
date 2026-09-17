/// Applications catalog view model.
///
/// Everything the screen does — first load, pull-to-refresh, append, error
/// handling, stale-response guarding — lives here as a platform-neutral state
/// machine that the Flutter widget only renders. The record->row mapping itself
/// is a pure injected function in `models/application_record_mapping.dart`.
library;

import 'package:sdkwork_webserver_flutter_mobile_commons/sdkwork_webserver_flutter_mobile_commons.dart';

import '../copy/applications_messages.dart';
import '../models/application_models.dart';
import '../models/application_record_mapping.dart';
import '../services/application_catalog_service.dart';

/// Key rendered for a token no closed-set label table covers.
const String unknownApplicationsLabelMessageKey = 'applications.list.row.unknown';

WebserverFlutterApplicationsScreenModel _createInitialState(
  String Function(String key) resolveMessage,
) {
  return WebserverFlutterApplicationsScreenModel(
    rows: const <WebserverFlutterApplicationRow>[],
    loading: true,
    appending: false,
    hasMore: false,
    errorMessage: '',
    errorDetail: '',
    page: 1,
    totalItems: 0,
    emptyMessage: resolveMessage('applications.list.empty'),
    loadingMessage: resolveMessage('applications.list.loading'),
    loadingMoreMessage: resolveMessage('applications.list.loadingMore'),
    retryLabel: resolveMessage('applications.list.retry'),
    loadMoreLabel: resolveMessage('applications.list.loadMore'),
  );
}

class WebserverFlutterApplicationsCatalogViewModel {
  WebserverFlutterApplicationsCatalogViewModel({
    required this.service,
    required this.resolveMessage,
    this.onStateChange,
    this.onSettled,
    this.pageSize,
  }) : _model = _createInitialState(resolveMessage);

  final WebserverFlutterApplicationsService service;
  final String Function(String key) resolveMessage;

  /// Receives the complete next state object.
  final void Function(WebserverFlutterApplicationsScreenModel state)? onStateChange;

  /// Called once the platform gesture behind a load may be released.
  final void Function()? onSettled;

  final int? pageSize;

  WebserverFlutterApplicationsScreenModel _model;

  /// Monotonic request id.
  ///
  /// A refresh issued while a page is in flight must not let the older response
  /// land on top of the newer one, and a logout mid-request must be able to
  /// invalidate it — both reduce to "the response id no longer matches".
  int _activeRequest = 0;

  WebserverFlutterApplicationsScreenModel get state => _model;

  void _apply(WebserverFlutterApplicationsScreenModel next) {
    _model = next;
    onStateChange?.call(next);
  }

  Future<void> _fetchPage(int page, bool append) async {
    _activeRequest += 1;
    final requestId = _activeRequest;
    _apply(
      append
          ? _model.copyWith(
              appending: true,
              errorMessage: '',
              errorDetail: '',
            )
          : _model.copyWith(
              loading: true,
              appending: false,
              errorMessage: '',
              errorDetail: '',
            ),
    );
    try {
      final result = await service.loadPage(page, pageSizeOverride: pageSize);
      if (requestId != _activeRequest) {
        return;
      }
      final source = append
          ? <WebserverFlutterApplicationItem>[
              ..._model.rows.map((row) => row.item),
              ...result.items,
            ]
          : result.items;
      final rows = source
          .map(
            (item) => toWebserverFlutterApplicationRow(
              item,
              webserverFlutterApplicationKindLabels,
              webserverFlutterApplicationStatusLabels,
              resolveMessage,
              unknownApplicationsLabelMessageKey,
            ),
          )
          .toList(growable: false);
      _apply(
        _model.copyWith(
          rows: rows,
          loading: false,
          appending: false,
          hasMore: result.page.hasMore,
          page: result.page.page,
          totalItems: result.page.totalItems,
        ),
      );
    } catch (error) {
      if (requestId != _activeRequest) {
        return;
      }
      // A missing transport is a different fact from a failed request, and it is
      // the screen's job to say which one happened: "we could not reach the
      // server" invites a retry, "this build has no transport" does not.
      final unavailable = error is WebserverDeployAppCatalogUnavailableError;
      _apply(
        _model.copyWith(
          loading: false,
          appending: false,
          errorMessage: resolveMessage(
            unavailable
                ? 'applications.list.unavailable'
                : 'applications.list.error',
          ),
          errorDetail: error.toString(),
        ),
      );
    } finally {
      if (requestId == _activeRequest) {
        onSettled?.call();
      }
    }
  }

  /// First page, or a fresh reload after pull-to-refresh.
  Future<void> refresh() async {
    _apply(
      _model.copyWith(
        rows: const <WebserverFlutterApplicationRow>[],
        page: 1,
        hasMore: false,
        totalItems: 0,
      ),
    );
    await _fetchPage(1, false);
  }

  /// Next page when one exists and no request is already in flight.
  Future<void> loadMore() async {
    if (_model.loading || _model.appending || !_model.hasMore) {
      return;
    }
    await _fetchPage(_model.page + 1, true);
  }

  WebserverFlutterScreenStatus status() => resolveWebserverFlutterScreenStatus(
    _model.rows.length,
    _model.loading,
    _model.errorMessage,
  );
}
