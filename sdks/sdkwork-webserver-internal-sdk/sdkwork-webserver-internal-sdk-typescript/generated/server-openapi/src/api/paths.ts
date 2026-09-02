export const CUSTOM_API_PREFIX = '/internal/v3/api';

export function customApiPath(path: string): string {
  if (!path) {
    return CUSTOM_API_PREFIX;
  }
  if (/^https?:\/\//i.test(path)) {
    return path;
  }
  const normalizedPrefixRaw = (CUSTOM_API_PREFIX || '').trim();
  const normalizedPrefix = normalizedPrefixRaw
    ? `/${normalizedPrefixRaw.replace(/^\/+|\/+$/g, '')}`
    : '';
  const normalizedPath = path.startsWith('/') ? path : `/${path}`;

  if (!normalizedPrefix || normalizedPrefix === '/') {
    return normalizedPath;
  }
  if (normalizedPath === normalizedPrefix || normalizedPath.startsWith(`${normalizedPrefix}/`)) {
    return normalizedPath;
  }
  return `${normalizedPrefix}${normalizedPath}`;
}
