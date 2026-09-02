import type { EnvVariableResponse } from './env-variable-response';

export interface EnvVariablePage {
  items?: EnvVariableResponse[];
  /** Total item count as a string to avoid JavaScript precision loss. */
  total?: string;
}
