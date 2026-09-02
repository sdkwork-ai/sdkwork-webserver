import type { SdkworkCustomConfig } from '../types/common';
import type { RequestOptions, QueryParams } from '@sdkwork/sdk-common';
import { BaseHttpClient, buildAuthHeaders, withRetry } from '@sdkwork/sdk-common';
import { sha256Hash } from '@sdkwork/utils';

type SdkworkV3UnwrapKind = 'item' | 'page' | 'command' | 'data' | 'void';

export type HttpRequestOptions = RequestOptions & {
  method?: string;
  body?: unknown;
  headers?: Record<string, string>;
  contentType?: string;
  accessTokenOnly?: boolean;
  sdkworkUnwrapKind?: SdkworkV3UnwrapKind;
};

export type ApiRequestOptions = Pick<HttpRequestOptions, 'signal' | 'timeout'>;

export class HttpClient extends BaseHttpClient {
  private static readonly API_KEY_HEADER: string = 'X-API-Key';
  private static readonly API_KEY_USE_BEARER = false;
  private static readonly SDKWORK_V3_UNWRAP = true;
  private static readonly SDKWORK_V3_REQUEST_FINGERPRINTS = true;

  constructor(config: SdkworkCustomConfig) {
    super(config as any);
    const initialApiKey = HttpClient.normalizeCredential((config as any).apiKey);
    if (initialApiKey) {
      this.setApiKey(initialApiKey);
    }
  }

  private static normalizeCredential(value: unknown): string | undefined {
    return typeof value === 'string' && value.trim().length > 0 ? value.trim() : undefined;
  }

  private getInternalAuthConfig(): any {
    const self = this as any;
    self.authConfig = self.authConfig || {};
    return self.authConfig;
  }
  private buildRequestHeaders(
    headers?: Record<string, string>,
    contentType?: string,
  ): Record<string, string> | undefined {
    const mergedHeaders = {
      ...(headers ?? {}),
    };

    if (contentType && contentType.toLowerCase() !== 'multipart/form-data') {
      mergedHeaders['Content-Type'] = contentType;
    }

    return Object.keys(mergedHeaders).length > 0 ? mergedHeaders : undefined;
  }

  private async applySdkworkRequestBodyFingerprint(
    headers: Record<string, string> | undefined,
    body: unknown,
  ): Promise<Record<string, string> | undefined> {
    if (
      !HttpClient.SDKWORK_V3_REQUEST_FINGERPRINTS
      || body == null
      || !this.hasNonEmptyHeader(headers, 'Idempotency-Key')
      || this.hasNonEmptyHeader(headers, 'X-Content-SHA256')
      || this.hasNonEmptyHeader(headers, 'X-Idempotency-Fingerprint')
    ) {
      return headers;
    }

    const fingerprint = await this.createSdkworkRequestBodyFingerprint(body);
    if (!fingerprint) {
      return headers;
    }

    const normalizedFingerprintHeader = fingerprint.header.toLowerCase();
    const preparedHeaders = Object.fromEntries(
      Object.entries(headers ?? {}).filter(
        ([headerName]) => headerName.toLowerCase() !== normalizedFingerprintHeader,
      ),
    );
    return {
      ...preparedHeaders,
      [fingerprint.header]: fingerprint.value,
    };
  }

  private hasNonEmptyHeader(headers: Record<string, string> | undefined, name: string): boolean {
    const normalizedName = name.toLowerCase();
    return Object.entries(headers ?? {}).some(
      ([headerName, value]) => headerName.toLowerCase() === normalizedName && value.trim().length > 0,
    );
  }

  private async createSdkworkRequestBodyFingerprint(
    body: unknown,
  ): Promise<{ header: 'X-Content-SHA256' | 'X-Idempotency-Fingerprint'; value: string } | undefined> {
    if (typeof FormData !== 'undefined' && body instanceof FormData) {
      const canonicalForm = await this.serializeSdkworkFormData(body);
      return {
        header: 'X-Idempotency-Fingerprint',
        value: await this.sha256Hex(new TextEncoder().encode(canonicalForm)),
      };
    }

    const bytes = await this.serializeSdkworkRequestBodyBytes(body);
    if (!bytes) {
      return undefined;
    }
    return {
      header: 'X-Content-SHA256',
      value: await this.sha256Hex(bytes),
    };
  }

  private async serializeSdkworkRequestBodyBytes(body: unknown): Promise<Uint8Array | undefined> {
    if (typeof URLSearchParams !== 'undefined' && body instanceof URLSearchParams) {
      return new TextEncoder().encode(body.toString());
    }
    if (typeof Blob !== 'undefined' && body instanceof Blob) {
      return new Uint8Array(await body.arrayBuffer());
    }
    if (typeof ArrayBuffer !== 'undefined' && body instanceof ArrayBuffer) {
      return new Uint8Array(body.slice(0));
    }
    if (typeof ArrayBuffer !== 'undefined' && ArrayBuffer.isView(body)) {
      return new Uint8Array(new Uint8Array(body.buffer, body.byteOffset, body.byteLength));
    }
    if (typeof body === 'string') {
      return new TextEncoder().encode(body);
    }

    const serialized = JSON.stringify(body);
    return serialized === undefined ? undefined : new TextEncoder().encode(serialized);
  }

  private async serializeSdkworkFormData(body: FormData): Promise<string> {
    const parts: Array<Record<string, unknown>> = [];
    for (const [name, value] of body.entries()) {
      if (typeof value === 'string') {
        parts.push({ kind: 'field', name, value });
        continue;
      }

      const bytes = new Uint8Array(await value.arrayBuffer());
      parts.push({
        kind: 'file',
        name,
        fileName: 'name' in value ? String(value.name) : '',
        contentType: value.type,
        size: value.size,
        contentSha256: await this.sha256Hex(bytes),
      });
    }
    return JSON.stringify(parts);
  }

  private async sha256Hex(bytes: Uint8Array): Promise<string> {
    return sha256Hash(bytes);
  }
  protected override buildHeaders(config: any, skipAuth = false): Record<string, string> {
    const headers = super.buildHeaders(config, true);
    if (!skipAuth && !config?.skipAuth) {
      const apiKey = this.getInternalAuthConfig().apiKey;
      if (typeof apiKey === 'string' && apiKey.length > 0) {
        headers[HttpClient.API_KEY_HEADER] = HttpClient.API_KEY_USE_BEARER
          ? `Bearer ${apiKey}`
          : apiKey;
      }
    }
    return headers;
  }
  private buildRequestBody(body: unknown, contentType?: string): unknown {
    if (body == null) {
      return body;
    }

    const normalizedContentType = (contentType ?? '').toLowerCase();
    if (normalizedContentType === 'application/x-www-form-urlencoded') {
      return this.encodeFormBody(body);
    }
    if (normalizedContentType === 'multipart/form-data') {
      return this.encodeMultipartBody(body);
    }

    return body;
  }

  private encodeMultipartBody(body: unknown): FormData {
    if (body instanceof FormData) {
      return body;
    }

    const formData = new FormData();
    if (body instanceof Map) {
      for (const [key, value] of body.entries()) {
        this.appendMultipartValue(formData, String(key), value);
      }
      return formData;
    }
    if (typeof body === 'object') {
      const record = body as Record<string, unknown>;
      for (const [key, value] of Object.entries(record)) {
        if (this.isMultipartMetadataField(key)) {
          continue;
        }
        this.appendMultipartValue(formData, key, value, this.resolveMultipartFileName(record, key));
      }
      return formData;
    }

    this.appendMultipartValue(formData, 'value', body);
    return formData;
  }

  private appendMultipartValue(formData: FormData, key: string, value: unknown, fileName?: string): void {
    if (value == null) {
      return;
    }
    if (Array.isArray(value)) {
      value.forEach((item) => this.appendMultipartValue(formData, key, item, fileName));
      return;
    }
    if (value instanceof Blob) {
      if (fileName) {
        formData.append(key, value, fileName);
        return;
      }
      formData.append(key, value);
      return;
    }
    if (value instanceof Date) {
      formData.append(key, value.toISOString());
      return;
    }
    if (typeof value === 'object') {
      formData.append(key, JSON.stringify(value));
      return;
    }
    formData.append(key, String(value));
  }

  private resolveMultipartFileName(record: Record<string, unknown>, key: string): string | undefined {
    const fieldSpecificName = record[`${key}FileName`];
    if (typeof fieldSpecificName === 'string' && fieldSpecificName.trim()) {
      return fieldSpecificName.trim();
    }
    const genericName = record.fileName;
    if (key === 'file' && typeof genericName === 'string' && genericName.trim()) {
      return genericName.trim();
    }
    return undefined;
  }

  private isMultipartMetadataField(key: string): boolean {
    return key === 'fileName' || key.endsWith('FileName');
  }

  private encodeFormBody(body: unknown): string {
    if (body instanceof URLSearchParams) {
      return body.toString();
    }
    if (typeof body === 'string') {
      return body;
    }

    const params = new URLSearchParams();
    if (body instanceof Map) {
      for (const [key, value] of body.entries()) {
        this.appendFormValue(params, String(key), value);
      }
      return params.toString();
    }
    if (typeof body === 'object') {
      for (const [key, value] of Object.entries(body as Record<string, unknown>)) {
        this.appendFormValue(params, key, value);
      }
      return params.toString();
    }

    params.append('value', String(body));
    return params.toString();
  }

  private appendFormValue(params: URLSearchParams, key: string, value: unknown): void {
    if (value == null) {
      return;
    }
    if (Array.isArray(value)) {
      value.forEach((item) => this.appendFormValue(params, key, item));
      return;
    }
    if (value instanceof Date) {
      params.append(key, value.toISOString());
      return;
    }
    if (typeof value === 'object') {
      params.append(key, JSON.stringify(value));
      return;
    }
    params.append(key, String(value));
  }
  override setApiKey(apiKey: string): void {
    this.getInternalAuthConfig().apiKey = apiKey;
  }
  private unwrapSdkworkV3Payload<T>(payload: unknown, unwrapKind: SdkworkV3UnwrapKind = 'data'): T {
    if (!HttpClient.SDKWORK_V3_UNWRAP || payload == null || typeof payload !== 'object') {
      return payload as T;
    }

    const record = payload as Record<string, unknown>;
    if (record.code !== 0 || !('data' in record)) {
      return this.unwrapSdkworkV3Data<T>(record, unwrapKind);
    }

    const data = record.data;
    if (!data || typeof data !== 'object') {
      return data as T;
    }

    return this.unwrapSdkworkV3Data<T>(data as Record<string, unknown>, unwrapKind);
  }

  private unwrapSdkworkV3Data<T>(data: Record<string, unknown>, unwrapKind: SdkworkV3UnwrapKind): T {
    if (unwrapKind === 'void') {
      return undefined as T;
    }
    if (unwrapKind === 'item' && 'item' in data) {
      return data.item as T;
    }

    return data as T;
  }

  override async request<T>(path: string, options: HttpRequestOptions = {}): Promise<T> {
    const execute = (this as any).execute;
    if (typeof execute !== 'function') {
      throw new Error('BaseHttpClient execute method is not available');
    }
    const {
      body,
      headers,
      contentType,
      method = 'GET',
      skipAuth,
      accessTokenOnly,
      sdkworkUnwrapKind = 'data',
      ...rest
    } = options;
    const requestHeaders = headers;
    const requestBody = this.buildRequestBody(body, contentType);
    const preparedHeaders = await this.applySdkworkRequestBodyFingerprint(
      this.buildRequestHeaders(requestHeaders, body == null ? undefined : contentType),
      requestBody,
    );
    const payload = await withRetry(
      () => execute.call(this, {
        url: path,
        method,
        ...rest,
        ...(skipAuth !== undefined ? { skipAuth } : {}),
        ...(accessTokenOnly !== undefined ? { accessTokenOnly } : {}),
        ...(requestBody !== undefined ? { body: requestBody } : {}),
        ...(preparedHeaders !== undefined ? { headers: preparedHeaders } : {}),
      }),
      // Per-request retry overrides (e.g. disabling 5xx retries for
      // idempotent-terminal operations like turn execution) flow through
      // options.retry; the default keeps maxRetries: 3.
      { maxRetries: 3, ...options.retry }
    );
    return this.unwrapSdkworkV3Payload<T>(payload, sdkworkUnwrapKind);
  }

  async *streamJson<T>(path: string, options: HttpRequestOptions = {}): AsyncIterable<T> {
    const stream = (BaseHttpClient.prototype as any).stream;
    if (typeof stream !== 'function') {
      throw new Error('BaseHttpClient stream method is not available');
    }
    const {
      body,
      headers,
      contentType,
      method = 'GET',
      skipAuth,
      accessTokenOnly,
      ...rest
    } = options;
    const authHeaders = headers;
    const requestBody = this.buildRequestBody(body, contentType);
    const requestHeaders = await this.applySdkworkRequestBodyFingerprint(
      this.buildRequestHeaders(
        { Accept: 'text/event-stream', ...(authHeaders ?? {}) },
        body == null ? undefined : contentType,
      ),
      requestBody,
    );

    for await (const data of stream.call(this, path, {
      method,
      ...rest,
      ...(skipAuth !== undefined ? { skipAuth } : {}),
      ...(accessTokenOnly !== undefined ? { accessTokenOnly } : {}),
      ...(requestBody !== undefined ? { body: requestBody } : {}),
      ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}),
    })) {
      if (data === '[DONE]') {
        return;
      }
      if (typeof data !== 'string' || data.trim().length === 0) {
        continue;
      }
      yield JSON.parse(data) as T;
    }
  }

  override async get<T>(path: string, params?: QueryParams, headers?: Record<string, string>): Promise<T> {
    return this.request<T>(path, {
      method: 'GET',
      ...(params !== undefined ? { params } : {}),
      ...(headers !== undefined ? { headers } : {}),
    });
  }

  override async post<T>(
    path: string,
    body?: unknown,
    params?: QueryParams,
    headers?: Record<string, string>,
    contentType?: string,
  ): Promise<T> {
    return this.request<T>(path, {
      method: 'POST',
      ...(body !== undefined ? { body } : {}),
      ...(params !== undefined ? { params } : {}),
      ...(headers !== undefined ? { headers } : {}),
      ...(contentType !== undefined ? { contentType } : {}),
    });
  }

  override async put<T>(
    path: string,
    body?: unknown,
    params?: QueryParams,
    headers?: Record<string, string>,
    contentType?: string,
  ): Promise<T> {
    return this.request<T>(path, {
      method: 'PUT',
      ...(body !== undefined ? { body } : {}),
      ...(params !== undefined ? { params } : {}),
      ...(headers !== undefined ? { headers } : {}),
      ...(contentType !== undefined ? { contentType } : {}),
    });
  }

  override async delete<T>(path: string, params?: QueryParams, headers?: Record<string, string>): Promise<T> {
    return this.request<T>(path, {
      method: 'DELETE',
      ...(params !== undefined ? { params } : {}),
      ...(headers !== undefined ? { headers } : {}),
    });
  }

  override async patch<T>(
    path: string,
    body?: unknown,
    params?: QueryParams,
    headers?: Record<string, string>,
    contentType?: string,
  ): Promise<T> {
    return this.request<T>(path, {
      method: 'PATCH',
      ...(body !== undefined ? { body } : {}),
      ...(params !== undefined ? { params } : {}),
      ...(headers !== undefined ? { headers } : {}),
      ...(contentType !== undefined ? { contentType } : {}),
    });
  }
}

export function createHttpClient(config: SdkworkCustomConfig): HttpClient {
  return new HttpClient(config);
}
