/**
 * Core TAMS domain types used across the web UI.
 */

/** TAMS flow_collection item per spec: id + role (both required). */
export interface FlowCollectionItem {
  id: string;
  role: string;
}

/** Extract flow IDs from a flow_collection array. */
export function collectionFlowIds(collection: FlowCollectionItem[] | undefined): string[] {
  return (collection || []).map(item => item.id);
}

export interface Bound {
  nanos: bigint;
  inclusive: boolean;
}

export interface TimeRangeParsed {
  type: 'range' | 'never' | 'eternity';
  start: Bound | null;
  end: Bound | null;
}

export interface Flow {
  id: string;
  label?: string;
  format: string;
  codec?: string;
  container?: string;
  source_id: string;
  essence_parameters?: Record<string, unknown>;
  tags?: Record<string, string | string[]>;
  flow_collection?: FlowCollectionItem[];
  timerange?: string;
  created?: string;
  updated?: string;
  description?: string;
  /** Client-side enriched duration in seconds */
  _duration?: number;
}

export interface Source {
  id: string;
  label?: string;
  format: string;
  description?: string;
  tags?: Record<string, string | string[]>;
  source_collection?: FlowCollectionItem[];
  collected_by?: string[];
  created_by?: string;
  updated_by?: string;
  created?: string;
  updated?: string;
}

export interface Segment {
  object_id: string;
  timerange: string;
  get_urls?: Array<{ url: string; presigned?: boolean; label?: string }>;
}

export interface PaginationInfo {
  limit: number | null;
  nextKey: string | null;
  count: number | null;
  timerange: string | null;
}

export interface ApiResponse<T = unknown> {
  data: T;
  status: number;
  headers: Headers;
}

export interface Toast {
  id: number;
  message: string;
  type: 'info' | 'error' | 'warning' | 'success';
  timeout: number;
}

// --- Ingest / Recording ---

export interface IngestSettings {
  videoCodec: string;
  audioCodec: string;
  videoQuality: string;
  audioQuality: string;
  segmentDuration: number;
  frameRate: string;
  keyFrameInterval: number;
}

export interface FrameRateOption {
  num: number;
  den: number;
  label: string;
}

export interface VideoCodecOption {
  id: string;
  label: string;
  tamsCodec: string;
  container: string;
}

export interface AudioCodecOption {
  id: string;
  label: string;
  tamsCodec: string;
  container: string;
}

export interface VideoQualityPreset {
  id: string;
  label: string;
  bitrate: number;
  iframeOnly?: boolean;
}

export interface AudioQualityPreset {
  id: string;
  label: string;
  bitrate: number;
}

// --- Assembly ---

export interface AssemblyItem {
  flow: Flow;
  audioFlows: Flow[];
}

export interface AssemblyResult {
  videoFlowId: string;
  audioFlowId: string | null;
  totalSegments: number;
  failed: number;
}

// --- Segment tracking ---

export interface SegmentEntry {
  index: number;
  flowType: string;
  timerange: string;
  status: 'pending' | 'uploading' | 'done' | 'failed';
  bytes: number;
  objectId: string | null;
  startedAt: number | null;
}
