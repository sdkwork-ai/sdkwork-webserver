import type { SdkworkAuthRuntimeConfig } from "@sdkwork/auth-pc-react";
import {
  isSdkworkAuthLoginMethod,
  isSdkworkAuthRecoveryMethod,
  isSdkworkAuthRegisterMethod,
  resolveSdkworkAuthRuntimeConfigFromMetadata,
  type SdkworkAuthVerificationPolicyConfig,
  type SdkworkCanonicalAuthMetadataLike,
} from "@sdkwork/iam-contracts";
import { readBootstrapAccessTokenFromProcessEnv } from "@sdkwork/iam-credential-entry";
import { resetTokenManagerToBootstrapAccessToken } from "@sdkwork/iam-runtime";
import type { AuthTokenManager } from "@sdkwork/sdk-common";

type JsonRecord = Record<string, unknown>;

export interface WebserverAuthRuntimeMetadataClient {
  system: {
    iam: {
      runtime: {
        retrieve(): Promise<unknown>;
      };
      verificationPolicy: {
        retrieve(): Promise<unknown>;
      };
    };
  };
}

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function unwrapSdkData(value: unknown, source: string): JsonRecord {
  if (!isRecord(value)) {
    throw new Error(`${source} metadata is unavailable`);
  }
  if ("code" in value) {
    if (value.code !== 0 || !isRecord(value.data)) {
      throw new Error(`${source} metadata is invalid`);
    }
    return value.data;
  }
  return value;
}

function readRecord(record: JsonRecord, key: string): JsonRecord {
  return isRecord(record[key]) ? record[key] : {};
}

function readBoolean(record: JsonRecord, ...keys: string[]): boolean | undefined {
  for (const key of keys) {
    if (typeof record[key] === "boolean") {
      return record[key];
    }
  }
  return undefined;
}

function readRequiredBoolean(record: JsonRecord, source: string, ...keys: string[]): boolean {
  const value = readBoolean(record, ...keys);
  if (value === undefined) {
    throw new Error(`${source} is required`);
  }
  return value;
}

function readString(record: JsonRecord, key: string): string | undefined {
  const value = record[key];
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function readStringArray(record: JsonRecord, key: string): string[] | undefined {
  const value = record[key];
  if (!Array.isArray(value)) {
    return undefined;
  }
  return [...new Set(value
    .map((item) => typeof item === "string" ? item.trim() : "")
    .filter(Boolean))];
}

function readRequiredStringArray(record: JsonRecord, key: string, source: string): string[] {
  const values = readStringArray(record, key);
  if (!values) {
    throw new Error(`${source} is required`);
  }
  return values;
}

function readOAuthProviders(auth: JsonRecord, runtime: JsonRecord): string[] {
  const configured = readStringArray(auth, "oauthProviders");
  const providers = configured
    ?? readStringArray(readRecord(readRecord(runtime, "accountBinding"), "oauthLogin"), "allowedProviders")
    ?? [];
  return providers.filter((provider) => /^[A-Za-z0-9_-]{1,64}$/u.test(provider));
}

function resolveOAuthProviderRegion(auth: JsonRecord): "mainland" | "overseas" | undefined {
  const region = readString(auth, "oauthProviderRegion")?.toLowerCase();
  if (region === "mainland") {
    return "mainland";
  }
  if (region === "overseas" || region === "global") {
    return "overseas";
  }
  return undefined;
}

function resolveContactMethods(runtime: JsonRecord, policy: JsonRecord): Array<"email" | "phone"> {
  const runtimeContact = readRecord(readRecord(runtime, "accountBinding"), "contactBinding");
  const policyContact = readRecord(readRecord(policy, "accountBinding"), "contactBinding");
  const emailEnabled = readBoolean(policyContact, "emailEnabled")
    ?? readBoolean(runtimeContact, "emailEnabled")
    ?? false;
  const phoneEnabled = readBoolean(policyContact, "phoneEnabled")
    ?? readBoolean(runtimeContact, "phoneEnabled")
    ?? false;
  return [
    ...(emailEnabled ? ["email" as const] : []),
    ...(phoneEnabled ? ["phone" as const] : []),
  ];
}

function resolveVerificationPolicy(
  auth: JsonRecord,
  policy: JsonRecord,
): SdkworkAuthVerificationPolicyConfig {
  return {
    emailCodeLoginEnabled: readRequiredBoolean(
      policy,
      "IAM emailCodeLoginEnabled",
      "emailCodeLoginEnabled",
    ),
    emailRegistrationVerificationRequired: readRequiredBoolean(
      policy,
      "IAM emailRegistrationVerificationRequired",
      "emailRegistrationVerificationRequired",
      "emailRegisterVerificationRequired",
    ),
    oauthLoginEnabled: readRequiredBoolean(auth, "IAM oauthLoginEnabled", "oauthLoginEnabled"),
    phoneCodeLoginEnabled: readRequiredBoolean(
      policy,
      "IAM phoneCodeLoginEnabled",
      "phoneCodeLoginEnabled",
    ),
    phoneRegistrationVerificationRequired: readRequiredBoolean(
      policy,
      "IAM phoneRegistrationVerificationRequired",
      "phoneRegistrationVerificationRequired",
      "phoneRegisterVerificationRequired",
    ),
  };
}

export function resolveWebserverAuthRuntimeConfigFromMetadata(
  runtimeValue: unknown,
  verificationPolicyValue: unknown,
): SdkworkAuthRuntimeConfig {
  const runtime = unwrapSdkData(runtimeValue, "IAM runtime");
  const auth = runtime.auth;
  if (!isRecord(auth)) {
    throw new Error("IAM runtime auth metadata is required");
  }
  const policy = unwrapSdkData(verificationPolicyValue, "IAM verification policy");
  const verificationPolicy = resolveVerificationPolicy(auth, policy);
  const contactMethods = resolveContactMethods(runtime, policy);
  const registrationEnabled = readRequiredBoolean(
    policy,
    "IAM registrationEnabled",
    "registrationEnabled",
  );
  const qrLoginEnabled = readRequiredBoolean(policy, "IAM qrLoginEnabled", "qrLoginEnabled");
  const loginMethods = readRequiredStringArray(auth, "loginMethods", "IAM loginMethods")
    .filter(isSdkworkAuthLoginMethod);
  const configuredRegisterMethods = readStringArray(auth, "registerMethods")
    ?.filter(isSdkworkAuthRegisterMethod);
  const configuredRecoveryMethods = readStringArray(auth, "recoveryMethods")
    ?.filter(isSdkworkAuthRecoveryMethod);
  const supportsLocalCredentials = readBoolean(auth, "supportsLocalCredentials");
  const supportsSessionExchange = readBoolean(auth, "supportsSessionExchange");
  const sdkworkOAuthProviderEnabled = readBoolean(auth, "sdkworkOAuthProviderEnabled");
  const metadata: SdkworkCanonicalAuthMetadataLike = {
    loginMethods,
    oauthLoginEnabled: verificationPolicy.oauthLoginEnabled,
    oauthProviders: readOAuthProviders(auth, runtime),
    qrLoginEnabled,
    recoveryMethods: configuredRecoveryMethods ?? contactMethods,
    registerMethods: registrationEnabled ? (configuredRegisterMethods ?? contactMethods) : [],
    ...(typeof sdkworkOAuthProviderEnabled === "boolean" ? { sdkworkOAuthProviderEnabled } : {}),
    ...(typeof supportsLocalCredentials === "boolean" ? { supportsLocalCredentials } : {}),
    ...(typeof supportsSessionExchange === "boolean" ? { supportsSessionExchange } : {}),
    verificationPolicy,
    ...(resolveOAuthProviderRegion(auth)
      ? { oauthProviderRegion: resolveOAuthProviderRegion(auth) }
      : {}),
  };
  const resolved = resolveSdkworkAuthRuntimeConfigFromMetadata(metadata);

  return {
    ...resolved,
    leftRailMode: qrLoginEnabled ? "qr-only" : "highlights-only",
    loginMethods,
    oauthLoginEnabled: verificationPolicy.oauthLoginEnabled,
    oauthProviders: [...(metadata.oauthProviders ?? [])],
    qrLoginEnabled,
    recoveryMethods: metadata.recoveryMethods?.filter(isSdkworkAuthRecoveryMethod) ?? [],
    registerMethods: registrationEnabled
      ? metadata.registerMethods?.filter(isSdkworkAuthRegisterMethod) ?? []
      : [],
    ...(typeof sdkworkOAuthProviderEnabled === "boolean" ? { sdkworkOAuthProviderEnabled } : {}),
    verificationPolicy,
  };
}

export function createWebserverAuthRuntimeConfigLoader(
  client: WebserverAuthRuntimeMetadataClient,
  tokenManager?: AuthTokenManager,
): () => Promise<SdkworkAuthRuntimeConfig> {
  let currentRequest: Promise<SdkworkAuthRuntimeConfig> | null = null;

  const prepareCredentialEntryBootstrapAccessToken = () => {
    if (!tokenManager) {
      return;
    }
    // Login metadata routes are access-token-only. A stale persisted Access-Token
    // from an expired session must not override the gateway-injected bootstrap
    // credential on /auth/login.
    resetTokenManagerToBootstrapAccessToken(
      tokenManager,
      readBootstrapAccessTokenFromProcessEnv(),
    );
  };

  return () => {
    prepareCredentialEntryBootstrapAccessToken();
    if (!currentRequest) {
      const request = Promise.all([
        client.system.iam.runtime.retrieve(),
        client.system.iam.verificationPolicy.retrieve(),
      ]).then(([runtime, verificationPolicy]) =>
        resolveWebserverAuthRuntimeConfigFromMetadata(runtime, verificationPolicy));
      currentRequest = request;
      void request.catch(() => {
        if (currentRequest === request) {
          currentRequest = null;
        }
      });
    }
    return currentRequest;
  };
}
