import {
  resolveAuthRedirectTarget,
  useSdkworkAuthControllerState,
  type SdkworkAuthController,
} from "@sdkwork/auth-pc-react";
import { useEffect, useState, type ReactNode } from "react";
import { Navigate, useLocation } from "react-router-dom";
import { resolveWebserverAuthHostMessages } from "./messages.ts";
import { WebserverAuthStatus } from "./WebserverAuthStatus.tsx";

type BootstrapStatus = "loading" | "ready" | "unavailable";

export function WebserverAuthGate({
  authRoutes,
  children,
  controller,
  locale,
}: {
  authRoutes: ReactNode;
  children: ReactNode;
  controller: SdkworkAuthController;
  locale: string;
}) {
  const location = useLocation();
  const state = useSdkworkAuthControllerState(controller);
  const [attempt, setAttempt] = useState(0);
  const [bootstrapStatus, setBootstrapStatus] = useState<BootstrapStatus>(
    state.isBootstrapped ? "ready" : "loading",
  );
  const messages = resolveWebserverAuthHostMessages(locale);
  const onAuthRoute = location.pathname === "/auth" || location.pathname.startsWith("/auth/");

  useEffect(() => {
    if (state.isBootstrapped) {
      setBootstrapStatus("ready");
      return undefined;
    }

    let active = true;
    setBootstrapStatus("loading");
    void controller.bootstrap()
      .then(() => {
        if (active) {
          setBootstrapStatus("ready");
        }
      })
      .catch((error: unknown) => {
        // Soft-fail like the public portal: treat bootstrap failure as
        // unauthenticated so the user reaches /auth/login (or shell) instead of
        // a dead "identity unavailable" wall. IAM metadata still needs the
        // credential-entry bootstrap token on the login page.
        console.error("Failed to bootstrap the IAM session.", error);
        if (active) {
          setBootstrapStatus("ready");
        }
      });
    return () => {
      active = false;
    };
  }, [attempt, controller, state.isBootstrapped]);

  if (bootstrapStatus === "loading") {
    return <WebserverAuthStatus message={messages.sessionChecking} />;
  }
  if (bootstrapStatus === "unavailable") {
    return (
      <WebserverAuthStatus
        homeHref="/"
        homeLabel={messages.backToPortal}
        message={messages.sessionUnavailable}
        onRetry={() => setAttempt((current) => current + 1)}
        retryLabel={messages.retry}
      />
    );
  }
  if (onAuthRoute && state.isAuthenticated) {
    const redirectTarget = resolveAuthRedirectTarget(
      new URLSearchParams(location.search).get("redirect"),
      "/",
      "/auth",
    );
    return <Navigate to={redirectTarget} replace />;
  }
  if (onAuthRoute) return <>{authRoutes}</>;
  if (!state.isAuthenticated) return <Navigate to={`/auth/login?redirect=${encodeURIComponent(`${location.pathname}${location.search}`)}`} replace />;
  return <>{children}</>;
}
