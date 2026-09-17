import { useMemo } from "react";
import { BrowserRouter, Navigate, Route, Routes } from "react-router-dom";

import { ApplicationsListView } from "@sdkwork/webserver-h5-applications";
import { StateBlock } from "@sdkwork/webserver-h5-commons";
import {
  APPLICATIONS_PATH,
  WEBSERVER_H5_APPLICATIONS_NAVIGATION,
  WEBSERVER_H5_HOME_PATH,
  WEBSERVER_H5_SHELL_NAVIGATION,
  WebserverH5AppShell,
  createWebserverH5Navigation,
} from "@sdkwork/webserver-h5-shell";

import type { WebserverH5ScreenClients } from "./bootstrap/sdkClients.ts";
import type { WebserverH5MessageResolver } from "./i18n/index.ts";

export interface AppProps {
  /** Clients built once at bootstrap; the root injects, features consume. */
  readonly clients: WebserverH5ScreenClients;
  readonly resolveMessage: WebserverH5MessageResolver;
}

/**
 * The H5 application root.
 *
 * All three root responsibilities live here and nowhere else: the shell is
 * assembled from the navigation model, the message catalog is projected into a
 * single resolver, and every feature screen receives the generated client it
 * needs. Feature packages own their screens; the root owns routing, locale, and
 * composition.
 */
export function App({ clients, resolveMessage }: AppProps) {
  const navigation = useMemo(
    () =>
      createWebserverH5Navigation([
        WEBSERVER_H5_SHELL_NAVIGATION,
        WEBSERVER_H5_APPLICATIONS_NAVIGATION,
      ]),
    [],
  );

  return (
    <BrowserRouter>
      <WebserverH5AppShell
        brand={resolveMessage("chrome.brand")}
        navigation={navigation}
        navigationLabel={resolveMessage("chrome.navigation.label")}
        resolveLabel={(entry) => resolveMessage(entry.labelKey)}
      >
        <Routes>
          <Route
            path={APPLICATIONS_PATH}
            element={
              <ApplicationsListView
                deployClient={clients.deploy}
                resolveMessage={resolveMessage}
              />
            }
          />
          <Route path="*" element={<Navigate to={WEBSERVER_H5_HOME_PATH} replace />} />
        </Routes>
      </WebserverH5AppShell>
    </BrowserRouter>
  );
}

/** Rendered while the runtime configuration is still in flight. */
export function WebserverH5BootstrapStatus({
  resolveMessage,
}: {
  readonly resolveMessage: WebserverH5MessageResolver;
}) {
  return <StateBlock tone="loading" message={resolveMessage("shell.bootstrap.loading")} />;
}

/**
 * Rendered when the runtime configuration cannot be loaded. The root surfaces
 * the failure instead of falling back to a guessed host: a renderer that
 * silently targets the wrong origin is harder to diagnose than one that refuses
 * to boot.
 */
export function WebserverH5BootstrapFailure({
  detail,
  resolveMessage,
}: {
  readonly detail?: string;
  readonly resolveMessage: WebserverH5MessageResolver;
}) {
  return (
    <>
      <StateBlock tone="error" message={resolveMessage("shell.bootstrap.failed")} />
      {detail ? <p className="h5-bootstrap__detail">{detail}</p> : null}
    </>
  );
}
