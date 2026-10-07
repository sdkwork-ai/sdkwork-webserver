import { i18n } from '@kit.LocalizationKit';

import {
  ApplicationsCatalogPage,
  createWebserverHarmonyApplicationsCatalogViewModel,
  createWebserverHarmonyApplicationsService,
  webserverHarmonyApplicationsMessagesEnUs,
  webserverHarmonyApplicationsMessagesZhCn,
  type WebserverHarmonyApplicationsScreenModel,
  type WebserverHarmonyApplicationsService,
} from '@sdkwork/sdkwork-webserver-harmony-mobile-applications';
import {
  getWebserverHarmonySdkClients,
  type WebserverHarmonySdkClients,
} from '../bootstrap/SdkClients';

/**
 * Root page.
 *
 * The root page stays a thin shell mount point: it resolves the composed SDK
 * port into the applications capability's view model and renders the
 * capability-owned screen (`HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §2 root
 * thinness). Business UI lives in the capability package.
 */
@Entry
@Component
struct Index {
  @State model: WebserverHarmonyApplicationsScreenModel | null = null;
  private viewModel: WebserverHarmonyApplicationsCatalogViewModel | null = null;

  aboutToAppear(): void {
    const clients = getWebserverHarmonySdkClients();
    // Locale negotiation once at composition: the shipped zh-CN catalog must
    // engage on zh devices instead of silently serving en-US copy.
    const messages = i18n.System.getSystemLanguage().startsWith('zh')
      ? webserverHarmonyApplicationsMessagesZhCn
      : webserverHarmonyApplicationsMessagesEnUs;
    this.viewModel = createWebserverHarmonyApplicationsCatalogViewModel({
      service: createApplicationsService(clients),
      resolveMessage: (key: string) => {
        const value: string | undefined = messages.get(key);
        return value !== undefined && value.length > 0 ? value : key;
      },
      onStateChange: (state: WebserverHarmonyApplicationsScreenModel) => {
        this.model = state;
      },
    });
    // Not awaited: the view model owns its loading state and reports it
    // through onStateChange, so the first frame renders "loading" immediately.
    // The catch is a pure safety net — the view model resolves every fetch
    // failure into the error state — it just must not become an unhandled
    // ArkTS promise rejection.
    this.viewModel.refresh().catch(() => {});
  }

  build() {
    Column() {
      ApplicationsCatalogPage({ model: this.model ?? emptyModel() })
    }
    .width('100%')
    .height('100%')
  }
}

function emptyModel(): WebserverHarmonyApplicationsScreenModel {
  return {
    rows: [],
    loading: true,
    appending: false,
    hasMore: false,
    errorMessage: '',
    errorDetail: '',
    page: 1,
    totalItems: 0,
    emptyMessage: '',
    loadingMessage: '',
    loadingMoreMessage: '',
    retryLabel: '',
    loadMoreLabel: '',
  };
}

/// Binds the composed deploy_app SDK port to the capability-owned service.
/// The port's `list(page, pageSize)` satisfies the reader interface
/// structurally; the capability package owns everything past the boundary.
function createApplicationsService(
  clients: WebserverHarmonySdkClients,
): WebserverHarmonyApplicationsService {
  return createWebserverHarmonyApplicationsService(clients.deployApp);
}
