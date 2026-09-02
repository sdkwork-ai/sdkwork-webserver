export interface ProblemDetail {
  type: string;
  title: string;
  status: number;
  detail?: string;
  code: number;
  traceId: string;
}
