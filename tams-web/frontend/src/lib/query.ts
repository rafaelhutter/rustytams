/** Apply tag/tag_exists filter params (shared across all query builders). */
function applyTagFilter(params: URLSearchParams, name: string, value: string, prefix: string = 'tag'): void {
  if (name?.trim() && value?.trim()) {
    params.set(`${prefix}.${name.trim()}`, value.trim());
  } else if (name?.trim()) {
    params.set(`${prefix}_exists.${name.trim()}`, 'true');
  }
}

/** Finalize URLSearchParams into a path with optional query string. */
function finalize(basePath: string, params: URLSearchParams): string {
  const qs = params.toString();
  return `${basePath}${qs ? '?' + qs : ''}`;
}

interface SourcesFilters {
  label?: string;
  format?: string;
  tagName?: string;
  tagValue?: string;
  limit?: number;
}

/**
 * Build a TAMS query string for the /sources endpoint.
 */
export function buildSourcesQuery(
  { label = '', format = '', tagName = '', tagValue = '', limit }: SourcesFilters = {},
  pageKey: string | null = null,
): string {
  const params = new URLSearchParams();
  if (label.trim()) params.set('label', label.trim());
  if (format) params.set('format', format);
  applyTagFilter(params, tagName, tagValue);
  if (limit !== undefined) params.set('limit', String(limit));
  if (pageKey) params.set('page', pageKey);
  return finalize('/sources', params);
}

interface FlowsFilters {
  label?: string;
  format?: string;
  codec?: string;
  sourceId?: string;
  timerange?: string;
  frameWidth?: string;
  frameHeight?: string;
  tagName?: string;
  tagValue?: string;
  limit?: number;
  includeTimerange?: boolean;
}

/**
 * Build a TAMS query string for GET /flows (list endpoint).
 */
export function buildFlowsQuery(
  {
    label = '', format = '', codec = '', sourceId = '',
    timerange = '', frameWidth = '', frameHeight = '',
    tagName = '', tagValue = '', limit, includeTimerange,
  }: FlowsFilters = {},
  pageKey: string | null = null,
): string {
  const params = new URLSearchParams();
  if (label.trim()) params.set('label', label.trim());
  if (format) params.set('format', format);
  if (codec.trim()) params.set('codec', codec.trim());
  if (sourceId.trim()) params.set('source_id', sourceId.trim());
  if (timerange.trim()) params.set('timerange', timerange.trim());
  if (frameWidth.trim()) params.set('frame_width', frameWidth.trim());
  if (frameHeight.trim()) params.set('frame_height', frameHeight.trim());
  applyTagFilter(params, tagName, tagValue);
  if (limit !== undefined) params.set('limit', String(limit));
  if (includeTimerange) params.set('include_timerange', 'true');
  if (pageKey) params.set('page', pageKey);
  return finalize('/flows', params);
}

interface FlowQueryOptions {
  includeTimerange?: boolean;
  timerange?: string;
}

/**
 * Build a TAMS query path for GET /flows/{flowId} (single flow).
 * - includeTimerange: include the flow's computed timerange in the response
 * - timerange: clip the returned timerange to the intersection with this range
 */
export function buildFlowQuery(
  flowId: string,
  { includeTimerange, timerange }: FlowQueryOptions = {},
): string {
  const params = new URLSearchParams();
  if (includeTimerange) params.set('include_timerange', 'true');
  if (timerange) params.set('timerange', timerange);
  return finalize(`/flows/${flowId}`, params);
}

/** Valid TAMS webhook event types. */
export const WEBHOOK_EVENTS: string[] = [
  'flows/created',
  'flows/updated',
  'flows/deleted',
  'flows/segments_added',
  'flows/segments_deleted',
  'sources/created',
  'sources/updated',
  'sources/deleted',
];

interface WebhooksFilters {
  tagName?: string;
  tagValue?: string;
  limit?: number;
}

/**
 * Build a TAMS query string for the /service/webhooks endpoint.
 */
export function buildWebhooksQuery(
  { tagName = '', tagValue = '', limit }: WebhooksFilters = {},
  pageKey: string | null = null,
): string {
  const params = new URLSearchParams();
  applyTagFilter(params, tagName, tagValue);
  if (limit !== undefined) params.set('limit', String(limit));
  if (pageKey) params.set('page', pageKey);
  return finalize('/service/webhooks', params);
}

interface ObjectQueryOptions {
  verboseStorage?: boolean;
  acceptGetUrls?: string;
  acceptStorageIds?: string;
  presigned?: boolean;
  flowTagName?: string;
  flowTagValue?: string;
  limit?: number;
  page?: string;
}

/**
 * Build a TAMS query path for GET /objects/{objectId}.
 */
export function buildObjectQuery(
  objectId: string,
  {
    verboseStorage,
    acceptGetUrls,
    acceptStorageIds,
    presigned,
    flowTagName,
    flowTagValue,
    limit,
    page,
  }: ObjectQueryOptions = {},
): string {
  const params = new URLSearchParams();
  if (verboseStorage) params.set('verbose_storage', 'true');
  if (acceptGetUrls) params.set('accept_get_urls', acceptGetUrls);
  if (acceptStorageIds) params.set('accept_storage_ids', acceptStorageIds);
  if (presigned !== undefined) params.set('presigned', String(presigned));
  applyTagFilter(params, flowTagName ?? '', flowTagValue ?? '', 'flow_tag');
  if (limit !== undefined) params.set('limit', String(limit));
  if (page) params.set('page', page);
  return finalize(`/objects/${objectId}`, params);
}

interface SegmentsQueryOptions {
  timerange?: string;
  objectId?: string;
  reverseOrder?: boolean;
  presigned?: boolean;
  acceptGetUrls?: string;
  acceptStorageIds?: string;
  verboseStorage?: boolean;
  includeObjectTimerange?: boolean;
  limit?: number;
  page?: string;
}

/**
 * Build a TAMS query path for GET /flows/{flowId}/segments.
 */
export function buildSegmentsQuery(
  flowId: string,
  {
    timerange,
    objectId,
    reverseOrder,
    presigned,
    acceptGetUrls,
    acceptStorageIds,
    verboseStorage,
    includeObjectTimerange,
    limit,
    page,
  }: SegmentsQueryOptions = {},
): string {
  const params = new URLSearchParams();
  if (timerange) params.set('timerange', timerange);
  if (objectId) params.set('object_id', objectId);
  if (reverseOrder) params.set('reverse_order', 'true');
  if (presigned !== undefined) params.set('presigned', String(presigned));
  if (acceptGetUrls) params.set('accept_get_urls', acceptGetUrls);
  if (acceptStorageIds) params.set('accept_storage_ids', acceptStorageIds);
  if (verboseStorage) params.set('verbose_storage', 'true');
  if (includeObjectTimerange) params.set('include_object_timerange', 'true');
  if (limit !== undefined) params.set('limit', String(limit));
  if (page) params.set('page', page);
  return finalize(`/flows/${flowId}/segments`, params);
}
