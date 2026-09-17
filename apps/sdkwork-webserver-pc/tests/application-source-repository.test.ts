import {
  isValidGitRepositoryUrl,
  normalizeGitRepositoryUrl,
} from "@sdkwork/webserver-pc-commons";
import { describe, expect, it } from "vitest";

describe("application Git repository source", () => {
  it("normalizes a valid HTTPS repository URL", () => {
    expect(normalizeGitRepositoryUrl(
      "  https://github.com/sdkwork/example.git  ",
    )).toBe("https://github.com/sdkwork/example.git");
  });

  it.each([
    "",
    "http://github.com/sdkwork/example.git",
    "https://user:secret@github.com/sdkwork/example.git",
    "https://github.com/sdkwork/example.git?token=secret",
    "https://github.com/sdkwork/example.git#main",
    "https://github.com/",
  ])("rejects an unsafe or incomplete repository URL: %s", (repository) => {
    expect(isValidGitRepositoryUrl(repository)).toBe(false);
    expect(() => normalizeGitRepositoryUrl(repository)).toThrow();
  });
});
