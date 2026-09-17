import type { WebserverApplicationRecord } from '@sdkwork/sdkwork-webserver-harmony-mobile-core';

import type {
  WebserverHarmonyApplicationItem,
  WebserverHarmonyApplicationRow,
} from './ApplicationModels';
import type { WebserverHarmonyEnumLabel } from '../i18n/ApplicationsMessages';

/**
 * Pure record -> view-model -> row mapping for the applications capability.
 *
 * Deliberately free of runtime imports: the label tables and the message resolver
 * arrive **injected**, so the whole mapping is a deterministic function that
 * `tests/harmony-surface-contract.test.mjs` loads straight from this source and
 * exercises without the HarmonyOS/ArkUI toolchain.
 */

/** Resolve a closed-set token onto its message key. */
export function lookupWebserverHarmonyEnumMessageKey(
  labels: WebserverHarmonyEnumLabel[],
  value: string,
  fallbackKey: string,
): string {
  for (const label of labels) {
    if (label.value === value) {
      return label.messageKey;
    }
  }
  return fallbackKey;
}

/**
 * Map one generated record onto the view model. Returns `null` for a record the
 * list cannot render (no identity), so a malformed row is dropped instead of
 * producing a blank card.
 */
export function mapWebserverHarmonyApplicationRecord(
  record: WebserverApplicationRecord | null | undefined,
): WebserverHarmonyApplicationItem | null {
  if (record === null || record === undefined) {
    return null;
  }
  const id: string = typeof record.id === 'string' ? record.id : '';
  if (id.length === 0) {
    return null;
  }
  const name: string = typeof record.name === 'string' && record.name.length > 0
    ? record.name
    : id;
  const platformTargetCount: number = Number.isFinite(record.platformTargetCount)
    ? record.platformTargetCount
    : 0;
  return {
    id,
    name,
    slug: typeof record.slug === 'string' ? record.slug : '',
    appKind: typeof record.appKind === 'string' ? record.appKind : '',
    appStatus: typeof record.appStatus === 'string' ? record.appStatus : '',
    description: typeof record.description === 'string' ? record.description : '',
    defaultEnvironment: typeof record.defaultEnvironment === 'string'
      ? record.defaultEnvironment
      : '',
    latestReleaseTag: typeof record.latestReleaseTag === 'string'
      ? record.latestReleaseTag
      : '',
    platformTargetCount,
    updatedAt: typeof record.updatedAt === 'string' ? record.updatedAt : '',
  };
}

/**
 * Attach the labels the ArkUI list renders.
 *
 * The enum lookup and the message resolution are both injected, so a new server
 * enum member degrades to `unknownFallbackKey` (a visible "Unknown" label) rather
 * than to a blank cell or a raw `SCREAMING_SNAKE` token.
 */
export function toWebserverHarmonyApplicationRow(
  item: WebserverHarmonyApplicationItem,
  kindLabels: WebserverHarmonyEnumLabel[],
  statusLabels: WebserverHarmonyEnumLabel[],
  resolveMessage: (key: string) => string,
  unknownFallbackKey: string,
): WebserverHarmonyApplicationRow {
  return {
    ...item,
    kindLabel: resolveMessage(
      lookupWebserverHarmonyEnumMessageKey(kindLabels, item.appKind, unknownFallbackKey),
    ),
    statusLabel: resolveMessage(
      lookupWebserverHarmonyEnumMessageKey(statusLabels, item.appStatus, unknownFallbackKey),
    ),
  };
}
