/**
 * Workspace lint gate for the browser clients.
 *
 * Scope is deliberately narrow: React hook correctness (rules of hooks as an
 * error, dependency completeness as a warning) on the authored PC/H5 sources.
 * Generated SDKs, `dist/`, `public/`, and declaration files are not authored
 * here. Style rules are intentionally absent — the repository formats with
 * `cargo fmt`/TypeScript and does not want two style authorities.
 */
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: [
      "**/dist/**",
      "**/node_modules/**",
      "**/generated/**",
      "**/public/**",
      "**/*.d.ts",
      "**/custom/**",
    ],
  },
  tseslint.configs.base,
  {
    files: [
      "apps/sdkwork-webserver-pc/src/**/*.{ts,tsx}",
      "apps/sdkwork-webserver-pc/packages/*/src/**/*.{ts,tsx}",
      "apps/sdkwork-webserver-h5/src/**/*.{ts,tsx}",
      "apps/sdkwork-webserver-h5/packages/*/src/**/*.{ts,tsx}",
    ],
    plugins: { "react-hooks": reactHooks },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "warn",
    },
  },
);
