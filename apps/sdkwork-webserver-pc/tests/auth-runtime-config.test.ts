import { describe, expect, it, vi } from "vitest";
import { createTokenManager } from "@sdkwork/sdk-common";
import {
  createWebserverAuthRuntimeConfigLoader,
  resolveWebserverAuthRuntimeConfigFromMetadata,
} from "../src/auth/authRuntimeConfig.ts";

const runtimeMetadata = {
  code: 0,
  data: {
    accountBinding: {
      contactBinding: {
        emailEnabled: true,
        phoneEnabled: true,
      },
      oauthLogin: {
        allowedProviders: ["github"],
        enabled: true,
      },
    },
    auth: {
      loginMethods: ["password", "emailCode", "sessionBridge"],
      oauthLoginEnabled: true,
      oauthProviderRegion: "global",
      sdkworkOAuthProviderEnabled: true,
      supportsLocalCredentials: true,
      supportsSessionExchange: true,
    },
  },
};

const verificationPolicyMetadata = {
  code: 0,
  data: {
    emailCodeLoginEnabled: true,
    emailRegistrationVerificationRequired: true,
    phoneCodeLoginEnabled: false,
    phoneRegistrationVerificationRequired: false,
    qrLoginEnabled: true,
    registrationEnabled: true,
  },
};

describe("webserver IAM runtime config", () => {
  it("maps canonical runtime capabilities without enabling undeclared methods", () => {
    const config = resolveWebserverAuthRuntimeConfigFromMetadata(
      runtimeMetadata,
      verificationPolicyMetadata,
    );

    expect(config.loginMethods).toEqual(["password", "emailCode", "sessionBridge"]);
    expect(config.oauthLoginEnabled).toBe(true);
    expect(config.oauthProviderRegion).toBe("overseas");
    expect(config.oauthProviders).toEqual(["github"]);
    expect(config.leftRailMode).toBe("qr-only");
    expect(config.qrLoginEnabled).toBe(true);
    expect(config.registerMethods).toEqual(["email", "phone"]);
    expect(config.recoveryMethods).toEqual(["email", "phone"]);
    expect(config.verificationPolicy).toEqual({
      emailCodeLoginEnabled: true,
      emailRegistrationVerificationRequired: true,
      oauthLoginEnabled: true,
      phoneCodeLoginEnabled: false,
      phoneRegistrationVerificationRequired: false,
    });
  });

  it("rejects incomplete policy metadata instead of inferring capabilities", () => {
    expect(() => resolveWebserverAuthRuntimeConfigFromMetadata(
      { code: 0, data: { auth: { loginMethods: ["password"] } } },
      { code: 0, data: { qrLoginEnabled: true } },
    )).toThrow(/IAM/);
  });

  it("re-seeds credential-entry bootstrap Access-Token before metadata SDK calls", async () => {
    const tokenManager = createTokenManager();
    tokenManager.setAccessToken("stale-access-token");
    vi.stubGlobal("__SDKWORK_CREDENTIAL_ENTRY_BOOTSTRAP_ACCESS_TOKEN__", "bootstrap-access-token");
    const runtimeRetrieve = vi.fn().mockResolvedValue(runtimeMetadata);
    const policyRetrieve = vi.fn().mockResolvedValue(verificationPolicyMetadata);
    const load = createWebserverAuthRuntimeConfigLoader({
      system: {
        iam: {
          runtime: { retrieve: runtimeRetrieve },
          verificationPolicy: { retrieve: policyRetrieve },
        },
      },
    }, tokenManager);

    await expect(load()).resolves.toMatchObject({ qrLoginEnabled: true });
    expect(tokenManager.getAccessToken()).toBe("bootstrap-access-token");
    vi.unstubAllGlobals();
  });

  it("clears a failed metadata request so retry performs fresh SDK calls", async () => {
    const runtimeRetrieve = vi.fn()
      .mockRejectedValueOnce(new Error("offline"))
      .mockResolvedValueOnce(runtimeMetadata);
    const policyRetrieve = vi.fn().mockResolvedValue(verificationPolicyMetadata);
    const load = createWebserverAuthRuntimeConfigLoader({
      system: {
        iam: {
          runtime: { retrieve: runtimeRetrieve },
          verificationPolicy: { retrieve: policyRetrieve },
        },
      },
    });

    await expect(load()).rejects.toThrow("offline");
    await expect(load()).resolves.toMatchObject({ qrLoginEnabled: true });
    expect(runtimeRetrieve).toHaveBeenCalledTimes(2);
    expect(policyRetrieve).toHaveBeenCalledTimes(2);
  });
});
