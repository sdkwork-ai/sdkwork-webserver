import { backendApiPath } from './paths';
import type { ApiRequestOptions, HttpClient } from '../http/client';

import type { ClusterEventResponse, ClusterHeartbeatSampleResponse, ClusterHostResponse, ClusterInstanceResponse, ClusterOverviewResponse, ClusterProbeRunResponse, ClusterResponse, ClusterSyncManifest, CreateClusterRequest, EnqueueClusterPeerMessagesRequest, EnqueueClusterPeerMessagesResponse, PageInfo, ProbeClusterInstanceRequest, PublishClusterSyncRequest, UpdateClusterHostRequest, UpdateClusterInstanceRequest, UpdateClusterRequest } from '../types';


export interface ClusterMessagesCreateParams {
  idempotencyKey: string;
}

export class ClusterMessagesApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Enqueue a peer message to one instance or broadcast to online members */
  async create(body: EnqueueClusterPeerMessagesRequest, params: ClusterMessagesCreateParams, requestOptions?: ApiRequestOptions): Promise<EnqueueClusterPeerMessagesResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<EnqueueClusterPeerMessagesResponse>(backendApiPath(`/clusters/messages`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }
}

export class ClusterOverviewApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** Retrieve the cluster health overview for status polling */
  async retrieve(requestOptions?: ApiRequestOptions): Promise<ClusterOverviewResponse> {
    return this.client.request<ClusterOverviewResponse>(backendApiPath(`/clusters/overview`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }
}

export interface ClusterEventsListParams {
  pageSize?: number;
  cursor?: string;
  clusterId?: string;
  severity?: 'INFO' | 'WARNING' | 'ERROR';
}

export class ClusterEventsApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** List cluster lifecycle events */
  async list(params?: ClusterEventsListParams, requestOptions?: ApiRequestOptions): Promise<{ items: ClusterEventResponse[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'page_size', value: params?.pageSize, style: 'form', explode: true, allowReserved: false },
      { name: 'cursor', value: params?.cursor, style: 'form', explode: true, allowReserved: false },
      { name: 'cluster_id', value: params?.clusterId, style: 'form', explode: true, allowReserved: false },
      { name: 'severity', value: params?.severity, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: ClusterEventResponse[]; pageInfo: PageInfo; }>(appendQueryString(backendApiPath(`/clusters/events`), query), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }
}

export interface ClusterInstancesMetricsListParams {
  limit?: number;
}

export class ClusterInstancesMetricsApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** List one instance's heartbeat metric samples for trend charts */
  async list(instanceId: string, params?: ClusterInstancesMetricsListParams, requestOptions?: ApiRequestOptions): Promise<{ items: ClusterHeartbeatSampleResponse[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'limit', value: params?.limit, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: ClusterHeartbeatSampleResponse[]; pageInfo: PageInfo; }>(appendQueryString(backendApiPath(`/clusters/instances/${serializePathParameter(instanceId, { name: 'instanceId', style: 'simple', explode: false })}/metrics/history`), query), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }
}

export interface ClusterInstancesHeartbeatsListParams {
  pageSize?: number;
  cursor?: string;
}

export class ClusterInstancesHeartbeatsApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** List one instance's stored heartbeat samples */
  async list(instanceId: string, params?: ClusterInstancesHeartbeatsListParams, requestOptions?: ApiRequestOptions): Promise<{ items: ClusterHeartbeatSampleResponse[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'page_size', value: params?.pageSize, style: 'form', explode: true, allowReserved: false },
      { name: 'cursor', value: params?.cursor, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: ClusterHeartbeatSampleResponse[]; pageInfo: PageInfo; }>(appendQueryString(backendApiPath(`/clusters/instances/${serializePathParameter(instanceId, { name: 'instanceId', style: 'simple', explode: false })}/heartbeats`), query), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }
}

export interface ClusterInstancesListParams {
  pageSize?: number;
  cursor?: string;
  clusterId?: string;
  hostId?: string;
  status?: number;
  healthState?: 'HEALTHY' | 'DEGRADED' | 'UNHEALTHY' | 'UNKNOWN';
}

export interface ClusterInstancesUpdateParams {
  idempotencyKey: string;
}

export interface ClusterInstancesDeleteParams {
  idempotencyKey: string;
}

export class ClusterInstancesApi {
  private client: HttpClient;
  public readonly heartbeats: ClusterInstancesHeartbeatsApi;
  public readonly metrics: ClusterInstancesMetricsApi;

  constructor(client: HttpClient) {
    this.client = client;
    this.heartbeats = new ClusterInstancesHeartbeatsApi(client);
    this.metrics = new ClusterInstancesMetricsApi(client);
  }


/** List webserver process instances with liveness state */
  async list(params?: ClusterInstancesListParams, requestOptions?: ApiRequestOptions): Promise<{ items: ClusterInstanceResponse[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'page_size', value: params?.pageSize, style: 'form', explode: true, allowReserved: false },
      { name: 'cursor', value: params?.cursor, style: 'form', explode: true, allowReserved: false },
      { name: 'cluster_id', value: params?.clusterId, style: 'form', explode: true, allowReserved: false },
      { name: 'host_id', value: params?.hostId, style: 'form', explode: true, allowReserved: false },
      { name: 'status', value: params?.status, style: 'form', explode: true, allowReserved: false },
      { name: 'health_state', value: params?.healthState, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: ClusterInstanceResponse[]; pageInfo: PageInfo; }>(appendQueryString(backendApiPath(`/clusters/instances`), query), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }

/** Retrieve a webserver process instance */
  async retrieve(instanceId: string, requestOptions?: ApiRequestOptions): Promise<ClusterInstanceResponse> {
    return this.client.request<ClusterInstanceResponse>(backendApiPath(`/clusters/instances/${serializePathParameter(instanceId, { name: 'instanceId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }

/** Update an instance display name, status, or advertised endpoint */
  async update(instanceId: string, body: UpdateClusterInstanceRequest, params: ClusterInstancesUpdateParams, requestOptions?: ApiRequestOptions): Promise<ClusterInstanceResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<ClusterInstanceResponse>(backendApiPath(`/clusters/instances/${serializePathParameter(instanceId, { name: 'instanceId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PATCH' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }

/** Unregister a webserver process instance */
  async delete(instanceId: string, params: ClusterInstancesDeleteParams, requestOptions?: ApiRequestOptions): Promise<void> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<void>(backendApiPath(`/clusters/instances/${serializePathParameter(instanceId, { name: 'instanceId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'DELETE' as any, ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}) });
  }

/** Probe one instance's connectivity and record the outcome */
  async probe(instanceId: string, body?: ProbeClusterInstanceRequest, requestOptions?: ApiRequestOptions): Promise<ClusterProbeRunResponse> {
    return this.client.request<ClusterProbeRunResponse>(backendApiPath(`/clusters/instances/${serializePathParameter(instanceId, { name: 'instanceId', style: 'simple', explode: false })}/probe`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, ...(body !== undefined ? { body, contentType: 'application/json' } : {}), sdkworkUnwrapKind: 'item' });
  }

/** Gracefully drain one instance out of routing */
  async drain(instanceId: string, requestOptions?: ApiRequestOptions): Promise<ClusterInstanceResponse> {
    return this.client.request<ClusterInstanceResponse>(backendApiPath(`/clusters/instances/${serializePathParameter(instanceId, { name: 'instanceId', style: 'simple', explode: false })}/drain`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, sdkworkUnwrapKind: 'item' });
  }

/** Clear the drain flag and restore routing participation */
  async undrain(instanceId: string, requestOptions?: ApiRequestOptions): Promise<ClusterInstanceResponse> {
    return this.client.request<ClusterInstanceResponse>(backendApiPath(`/clusters/instances/${serializePathParameter(instanceId, { name: 'instanceId', style: 'simple', explode: false })}/undrain`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, sdkworkUnwrapKind: 'item' });
  }

/** Cordon one instance out of routing without draining it */
  async cordon(instanceId: string, requestOptions?: ApiRequestOptions): Promise<ClusterInstanceResponse> {
    return this.client.request<ClusterInstanceResponse>(backendApiPath(`/clusters/instances/${serializePathParameter(instanceId, { name: 'instanceId', style: 'simple', explode: false })}/cordon`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, sdkworkUnwrapKind: 'item' });
  }

/** Uncordon one instance back into routing */
  async uncordon(instanceId: string, requestOptions?: ApiRequestOptions): Promise<ClusterInstanceResponse> {
    return this.client.request<ClusterInstanceResponse>(backendApiPath(`/clusters/instances/${serializePathParameter(instanceId, { name: 'instanceId', style: 'simple', explode: false })}/uncordon`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, sdkworkUnwrapKind: 'item' });
  }
}

export interface ClusterHostsListParams {
  pageSize?: number;
  cursor?: string;
  clusterId?: string;
  status?: number;
}

export interface ClusterHostsUpdateParams {
  idempotencyKey: string;
}

export interface ClusterHostsDeleteParams {
  idempotencyKey: string;
}

export class ClusterHostsApi {
  private client: HttpClient;

  constructor(client: HttpClient) {
    this.client = client;
  }


/** List cluster hosts with system and network identity */
  async list(params?: ClusterHostsListParams, requestOptions?: ApiRequestOptions): Promise<{ items: ClusterHostResponse[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'page_size', value: params?.pageSize, style: 'form', explode: true, allowReserved: false },
      { name: 'cursor', value: params?.cursor, style: 'form', explode: true, allowReserved: false },
      { name: 'cluster_id', value: params?.clusterId, style: 'form', explode: true, allowReserved: false },
      { name: 'status', value: params?.status, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: ClusterHostResponse[]; pageInfo: PageInfo; }>(appendQueryString(backendApiPath(`/clusters/hosts`), query), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }

/** Retrieve a cluster host */
  async retrieve(hostId: string, requestOptions?: ApiRequestOptions): Promise<ClusterHostResponse> {
    return this.client.request<ClusterHostResponse>(backendApiPath(`/clusters/hosts/${serializePathParameter(hostId, { name: 'hostId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }

/** Rename a host or reassign it to another cluster */
  async update(hostId: string, body: UpdateClusterHostRequest, params: ClusterHostsUpdateParams, requestOptions?: ApiRequestOptions): Promise<ClusterHostResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<ClusterHostResponse>(backendApiPath(`/clusters/hosts/${serializePathParameter(hostId, { name: 'hostId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PATCH' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }

/** Remove an instance-free host from the cluster inventory */
  async delete(hostId: string, params: ClusterHostsDeleteParams, requestOptions?: ApiRequestOptions): Promise<void> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<void>(backendApiPath(`/clusters/hosts/${serializePathParameter(hostId, { name: 'hostId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'DELETE' as any, ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}) });
  }
}

export interface ClusterListParams {
  page?: number;
  pageSize?: number;
}

export interface ClusterCreateParams {
  idempotencyKey: string;
}

export interface ClusterUpdateParams {
  idempotencyKey: string;
}

export interface ClusterDeleteParams {
  idempotencyKey: string;
}

export class ClusterApi {
  private client: HttpClient;
  public readonly hosts: ClusterHostsApi;
  public readonly instances: ClusterInstancesApi;
  public readonly events: ClusterEventsApi;
  public readonly overview: ClusterOverviewApi;
  public readonly messages: ClusterMessagesApi;

  constructor(client: HttpClient) {
    this.client = client;
    this.hosts = new ClusterHostsApi(client);
    this.instances = new ClusterInstancesApi(client);
    this.events = new ClusterEventsApi(client);
    this.overview = new ClusterOverviewApi(client);
    this.messages = new ClusterMessagesApi(client);
  }


/** List Web Server clusters */
  async list(params?: ClusterListParams, requestOptions?: ApiRequestOptions): Promise<{ items: ClusterResponse[]; pageInfo: PageInfo; }> {
    const query = buildQueryString([
      { name: 'page', value: params?.page, style: 'form', explode: true, allowReserved: false },
      { name: 'page_size', value: params?.pageSize, style: 'form', explode: true, allowReserved: false },
    ]);
    return this.client.request<{ items: ClusterResponse[]; pageInfo: PageInfo; }>(appendQueryString(backendApiPath(`/clusters`), query), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'page' });
  }

/** Create a Web Server cluster */
  async create(body: CreateClusterRequest, params: ClusterCreateParams, requestOptions?: ApiRequestOptions): Promise<ClusterResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<ClusterResponse>(backendApiPath(`/clusters`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }

/** Retrieve a Web Server cluster */
  async retrieve(clusterId: string, requestOptions?: ApiRequestOptions): Promise<ClusterResponse> {
    return this.client.request<ClusterResponse>(backendApiPath(`/clusters/${serializePathParameter(clusterId, { name: 'clusterId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'GET' as any, sdkworkUnwrapKind: 'item' });
  }

/** Update a Web Server cluster */
  async update(clusterId: string, body: UpdateClusterRequest, params: ClusterUpdateParams, requestOptions?: ApiRequestOptions): Promise<ClusterResponse> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<ClusterResponse>(backendApiPath(`/clusters/${serializePathParameter(clusterId, { name: 'clusterId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'PATCH' as any, body, contentType: 'application/json', ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}), sdkworkUnwrapKind: 'item' });
  }

/** Delete an empty Web Server cluster */
  async delete(clusterId: string, params: ClusterDeleteParams, requestOptions?: ApiRequestOptions): Promise<void> {
    const requestHeaders = buildRequestHeaders(
      {
        'Idempotency-Key': { value: params.idempotencyKey, style: 'simple', explode: false },
      },
      {}
    );
    return this.client.request<void>(backendApiPath(`/clusters/${serializePathParameter(clusterId, { name: 'clusterId', style: 'simple', explode: false })}`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'DELETE' as any, ...(requestHeaders !== undefined ? { headers: requestHeaders } : {}) });
  }

/** Publish a desired-state revision to every instance of the cluster */
  async sync(clusterId: string, body: PublishClusterSyncRequest, requestOptions?: ApiRequestOptions): Promise<ClusterSyncManifest> {
    return this.client.request<ClusterSyncManifest>(backendApiPath(`/clusters/${serializePathParameter(clusterId, { name: 'clusterId', style: 'simple', explode: false })}/sync`), { ...(requestOptions?.signal !== undefined ? { signal: requestOptions.signal } : {}), ...(requestOptions?.timeout !== undefined ? { timeout: requestOptions.timeout } : {}), method: 'POST' as any, body, contentType: 'application/json', sdkworkUnwrapKind: 'item' });
  }
}

export function createClusterApi(client: HttpClient): ClusterApi {
  return new ClusterApi(client);
}

function appendQueryString(path: string, rawQueryString: string): string {
  const query = rawQueryString.replace(/^\?+/, '');
  if (!query) {
    return path;
  }
  return path.includes('?') ? `${path}&${query}` : `${path}?${query}`;
}

interface PathParameterSpec {
  name: string;
  style: string;
  explode: boolean;
}

function serializePathParameter(value: unknown, spec: PathParameterSpec): string {
  if (value === undefined || value === null) {
    return '';
  }

  const style = spec.style || 'simple';
  if (Array.isArray(value)) {
    return serializePathArray(spec.name, value, style, spec.explode);
  }
  if (typeof value === 'object') {
    return serializePathObject(spec.name, value as Record<string, unknown>, style, spec.explode);
  }
  return pathPrefix(spec.name, style, false) + encodePathValue(serializePathPrimitive(value));
}

function serializePathArray(name: string, values: unknown[], style: string, explode: boolean): string {
  const serialized = values
    .filter((item) => item !== undefined && item !== null)
    .map((item) => encodePathValue(serializePathPrimitive(item)));
  if (serialized.length === 0) {
    return pathPrefix(name, style, false);
  }
  if (style === 'matrix') {
    return explode
      ? serialized.map((item) => `;${name}=${item}`).join('')
      : `;${name}=${serialized.join(',')}`;
  }
  return pathPrefix(name, style, false) + serialized.join(explode ? '.' : ',');
}

function serializePathObject(name: string, value: Record<string, unknown>, style: string, explode: boolean): string {
  const entries = Object.entries(value).filter(([, entryValue]) => entryValue !== undefined && entryValue !== null);
  if (entries.length === 0) {
    return pathPrefix(name, style, true);
  }
  if (style === 'matrix') {
    return explode
      ? entries.map(([key, entryValue]) => `;${encodePathValue(key)}=${encodePathValue(serializePathPrimitive(entryValue))}`).join('')
      : `;${name}=${entries.flatMap(([key, entryValue]) => [encodePathValue(key), encodePathValue(serializePathPrimitive(entryValue))]).join(',')}`;
  }
  const serialized = explode
    ? entries.map(([key, entryValue]) => `${encodePathValue(key)}=${encodePathValue(serializePathPrimitive(entryValue))}`).join(style === 'label' ? '.' : ',')
    : entries.flatMap(([key, entryValue]) => [encodePathValue(key), encodePathValue(serializePathPrimitive(entryValue))]).join(',');
  return pathPrefix(name, style, true) + serialized;
}

function pathPrefix(name: string, style: string, _objectValue: boolean): string {
  if (style === 'label') return '.';
  if (style === 'matrix') return `;${name}`;
  return '';
}

function encodePathValue(value: string): string {
  return encodeURIComponent(value);
}

function serializePathPrimitive(value: unknown): string {
  if (value instanceof Date) {
    return value.toISOString();
  }
  if (typeof value === 'object') {
    return JSON.stringify(value);
  }
  return String(value);
}
interface QueryParameterSpec {
  name: string;
  value: unknown;
  style: string;
  explode: boolean;
  allowReserved: boolean;
  contentType?: string;
}

function buildQueryString(parameters: QueryParameterSpec[]): string {
  const pairs: string[] = [];
  for (const parameter of parameters) {
    appendSerializedParameter(pairs, parameter);
  }
  return pairs.join('&');
}

function appendSerializedParameter(pairs: string[], parameter: QueryParameterSpec): void {
  if (parameter.value === undefined || parameter.value === null) {
    return;
  }

  if (parameter.contentType) {
    pairs.push(`${encodeQueryComponent(parameter.name)}=${encodeQueryValue(JSON.stringify(parameter.value), parameter.allowReserved)}`);
    return;
  }

  const style = parameter.style || 'form';
  if (style === 'deepObject') {
    appendDeepObjectParameter(pairs, parameter.name, parameter.value, parameter.allowReserved);
    return;
  }

  if (Array.isArray(parameter.value)) {
    appendArrayParameter(pairs, parameter.name, parameter.value, style, parameter.explode, parameter.allowReserved);
    return;
  }

  if (typeof parameter.value === 'object') {
    appendObjectParameter(pairs, parameter.name, parameter.value as Record<string, unknown>, style, parameter.explode, parameter.allowReserved);
    return;
  }

  pairs.push(`${encodeQueryComponent(parameter.name)}=${encodeQueryValue(serializePrimitive(parameter.value), parameter.allowReserved)}`);
}

function appendArrayParameter(
  pairs: string[],
  name: string,
  value: unknown[],
  style: string,
  explode: boolean,
  allowReserved: boolean,
): void {
  const values = value
    .filter((item) => item !== undefined && item !== null)
    .map((item) => serializePrimitive(item));
  if (values.length === 0) {
    return;
  }

  if (style === 'form' && explode) {
    for (const item of values) {
      pairs.push(`${encodeQueryComponent(name)}=${encodeQueryValue(item, allowReserved)}`);
    }
    return;
  }

  pairs.push(`${encodeQueryComponent(name)}=${encodeQueryValue(values.join(','), allowReserved)}`);
}

function appendObjectParameter(
  pairs: string[],
  name: string,
  value: Record<string, unknown>,
  style: string,
  explode: boolean,
  allowReserved: boolean,
): void {
  const entries = Object.entries(value).filter(([, entryValue]) => entryValue !== undefined && entryValue !== null);
  if (entries.length === 0) {
    return;
  }

  if (style === 'form' && explode) {
    for (const [key, entryValue] of entries) {
      pairs.push(`${encodeQueryComponent(key)}=${encodeQueryValue(serializePrimitive(entryValue), allowReserved)}`);
    }
    return;
  }

  const serialized = entries.flatMap(([key, entryValue]) => [key, serializePrimitive(entryValue)]).join(',');
  pairs.push(`${encodeQueryComponent(name)}=${encodeQueryValue(serialized, allowReserved)}`);
}

function appendDeepObjectParameter(
  pairs: string[],
  name: string,
  value: unknown,
  allowReserved: boolean,
): void {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    pairs.push(`${encodeQueryComponent(name)}=${encodeQueryValue(serializePrimitive(value), allowReserved)}`);
    return;
  }

  for (const [key, entryValue] of Object.entries(value as Record<string, unknown>)) {
    if (entryValue === undefined || entryValue === null) {
      continue;
    }
    pairs.push(`${encodeQueryComponent(`${name}[${key}]`)}=${encodeQueryValue(serializePrimitive(entryValue), allowReserved)}`);
  }
}

function serializePrimitive(value: unknown): string {
  if (value instanceof Date) {
    return value.toISOString();
  }
  if (typeof value === 'object') {
    return JSON.stringify(value);
  }
  return String(value);
}

function encodeQueryComponent(value: string): string {
  return encodeURIComponent(value);
}

function encodeQueryValue(value: string, allowReserved: boolean): string {
  const encoded = encodeURIComponent(value);
  if (!allowReserved) {
    return encoded;
  }
  return encoded.replace(/%3A/gi, ':')
    .replace(/%2F/gi, '/')
    .replace(/%3F/gi, '?')
    .replace(/%23/gi, '#')
    .replace(/%5B/gi, '[')
    .replace(/%5D/gi, ']')
    .replace(/%40/gi, '@')
    .replace(/%21/gi, '!')
    .replace(/%24/gi, '$')
    .replace(/%26/gi, '&')
    .replace(/%27/gi, "'")
    .replace(/%28/gi, '(')
    .replace(/%29/gi, ')')
    .replace(/%2A/gi, '*')
    .replace(/%2B/gi, '+')
    .replace(/%2C/gi, ',')
    .replace(/%3B/gi, ';')
    .replace(/%3D/gi, '=');
}
function buildRequestHeaders(
  headers: Record<string, HeaderParameterSpec | undefined>,
  cookies: Record<string, HeaderParameterSpec | undefined> = {},
): Record<string, string> | undefined {
  const requestHeaders: Record<string, string> = {};

  for (const [name, parameter] of Object.entries(headers)) {
    const serialized = serializeParameterValue(parameter);
    if (serialized !== undefined) {
      requestHeaders[name] = serialized;
    }
  }

  const cookieHeader = buildCookieHeader(cookies);
  if (cookieHeader) {
    requestHeaders.Cookie = requestHeaders.Cookie
      ? `${requestHeaders.Cookie}; ${cookieHeader}`
      : cookieHeader;
  }

  return Object.keys(requestHeaders).length > 0 ? requestHeaders : undefined;
}

interface HeaderParameterSpec {
  value: unknown;
  style: string;
  explode: boolean;
  contentType?: string;
}

function buildCookieHeader(cookies: Record<string, HeaderParameterSpec | undefined>): string | undefined {
  const pairs: string[] = [];
  for (const [name, parameter] of Object.entries(cookies)) {
    const serialized = serializeParameterValue(parameter);
    if (serialized !== undefined) {
      pairs.push(`${encodeURIComponent(name)}=${encodeURIComponent(serialized)}`);
    }
  }
  return pairs.length > 0 ? pairs.join('; ') : undefined;
}

function serializeParameterValue(parameter: HeaderParameterSpec | undefined): string | undefined {
  const value = parameter?.value;
  if (value === undefined || value === null) {
    return undefined;
  }
  if (parameter?.contentType) {
    return JSON.stringify(value);
  }
  if (value instanceof Date) {
    return value.toISOString();
  }
  if (Array.isArray(value)) {
    return value.map((item) => serializeHeaderPrimitive(item)).join(',');
  }
  if (typeof value === 'object' && value !== null) {
    return serializeHeaderObject(value as Record<string, unknown>, parameter?.explode === true);
  }
  return serializeHeaderPrimitive(value);
}

function serializeHeaderObject(value: Record<string, unknown>, explode: boolean): string {
  const entries = Object.entries(value).filter(([, entryValue]) => entryValue !== undefined && entryValue !== null);
  if (explode) {
    return entries.map(([key, entryValue]) => `${key}=${serializeHeaderPrimitive(entryValue)}`).join(',');
  }
  return entries.flatMap(([key, entryValue]) => [key, serializeHeaderPrimitive(entryValue)]).join(',');
}

function serializeHeaderPrimitive(value: unknown): string {
  if (value instanceof Date) {
    return value.toISOString();
  }
  return String(value);
}
