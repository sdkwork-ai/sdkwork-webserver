/**
 * Host-adapter registry for the H5 core. Browser hosts are the only adapter
 * this root supports; native hosts are served by the flutter / mini-program /
 * harmony roots, which own their own core packages.
 */
export function createSdkworkWebserverH5HostRegistry() {
  return {} as const;
}
