const GIT_REPOSITORY_MAX_LENGTH = 500;

/**
 * Validates an HTTPS Git repository URL and returns its trimmed form.
 *
 * Shared by every surface that accepts a Git source (console plugins today);
 * it deliberately rejects credentials, query strings, and fragments so the
 * value can be persisted and re-displayed without leaking secrets.
 */
export function normalizeGitRepositoryUrl(value: string | undefined): string {
  const repository = value?.trim();
  if (!repository) throw new Error("Git repository is required");
  if (repository.length > GIT_REPOSITORY_MAX_LENGTH) {
    throw new Error(`Git repository must not exceed ${GIT_REPOSITORY_MAX_LENGTH} characters`);
  }

  let parsed: URL;
  try {
    parsed = new URL(repository);
  } catch {
    throw new Error("Git repository must be a valid HTTPS URL");
  }
  if (
    parsed.protocol !== "https:"
    || !parsed.hostname
    || parsed.pathname === "/"
    || parsed.username
    || parsed.password
    || parsed.search
    || parsed.hash
  ) {
    throw new Error("Git repository must be an HTTPS URL without credentials, query parameters, or fragments");
  }
  return repository;
}

/** Non-throwing form of {@link normalizeGitRepositoryUrl} for live form validation. */
export function isValidGitRepositoryUrl(value: string | undefined): boolean {
  try {
    normalizeGitRepositoryUrl(value);
    return true;
  } catch {
    return false;
  }
}
