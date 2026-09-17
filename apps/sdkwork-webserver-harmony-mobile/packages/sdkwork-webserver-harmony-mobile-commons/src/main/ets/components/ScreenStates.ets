import { WebserverHarmonyTokens } from '../theme/DesignTokens';

/**
 * Domain-neutral empty/error/retry primitive.
 *
 * Payload-free on purpose so every capability can reuse it
 * (`HARMONY_APP_MOBILE_ARCHITECTURE_SPEC.md` §4 — commons owns the shared
 * primitives, capability packages own the pages).
 */
export type WebserverHarmonyScreenStatus = 'loading' | 'ready' | 'empty' | 'error';

/**
 * Collapse an item count, an in-flight flag, and an error message into the one
 * status every list screen renders. Pure and exported so the Node-side contract
 * test can exercise the state machine without the ArkUI runtime.
 */
export function resolveWebserverHarmonyScreenStatus(
  itemCount: number,
  loading: boolean,
  errorMessage: string,
): WebserverHarmonyScreenStatus {
  if (loading) {
    return 'loading';
  }
  if (errorMessage.length > 0) {
    return 'error';
  }
  return itemCount === 0 ? 'empty' : 'ready';
}

/**
 * Message key a given status renders. Keeping the mapping here means a new status
 * cannot be added without deciding what it says.
 */
export function resolveWebserverHarmonyScreenMessageKey(status: WebserverHarmonyScreenStatus): string {
  if (status === 'loading') return 'applications.list.loading';
  if (status === 'error') return 'applications.list.error';
  if (status === 'empty') return 'applications.list.empty';
  return '';
}

@Component
export struct WebserverHarmonyEmptyState {
  @Prop message: string = '';

  build() {
    Column() {
      Text(this.message)
        .fontSize(14)
        .fontColor(WebserverHarmonyTokens.colorTextMuted)
        .textAlign(TextAlign.Center)
    }
    .width('100%')
    .padding(WebserverHarmonyTokens.spacingLg)
    .justifyContent(FlexAlign.Center)
  }
}
