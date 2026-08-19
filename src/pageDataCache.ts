import type { GiftCardHistoryResponse, PlansResponse } from "./businessApi";
import type { NodeCatalogResponse, SubscriptionSnapshotResponse } from "./ipc";

export interface CachedPageResource<T> {
  value: T | null;
  inFlight: Promise<T> | null;
}

export interface SessionPageDataCache {
  userId: string;
  subscriptionSnapshot: CachedPageResource<SubscriptionSnapshotResponse>;
  plans: CachedPageResource<PlansResponse>;
  giftCardHistory: CachedPageResource<GiftCardHistoryResponse>;
  nodeCatalog: CachedPageResource<NodeCatalogResponse>;
  businessRequestTail: Promise<void>;
}

function createResource<T>(): CachedPageResource<T> {
  return { value: null, inFlight: null };
}

export function createSessionPageDataCache(
  userId: string,
): SessionPageDataCache {
  return {
    userId,
    subscriptionSnapshot: createResource(),
    plans: createResource(),
    giftCardHistory: createResource(),
    nodeCatalog: createResource(),
    businessRequestTail: Promise.resolve(),
  };
}

export function setCachedPageResource<T>(
  resource: CachedPageResource<T>,
  value: T,
): void {
  resource.value = value;
}

export function loadCachedPageResource<T>(
  resource: CachedPageResource<T>,
  load: () => Promise<T>,
  options?: { force?: boolean },
): Promise<T> {
  if (resource.inFlight !== null) return resource.inFlight;
  if (options?.force !== true && resource.value !== null) {
    return Promise.resolve(resource.value);
  }

  const request = load().then((value) => {
    resource.value = value;
    return value;
  });
  resource.inFlight = request;
  void request.then(
    () => {
      if (resource.inFlight === request) resource.inFlight = null;
    },
    () => {
      if (resource.inFlight === request) resource.inFlight = null;
    },
  );
  return request;
}

export function queueBusinessRequest<T>(
  cache: SessionPageDataCache,
  request: () => Promise<T>,
): Promise<T> {
  const result = cache.businessRequestTail.then(request, request);
  cache.businessRequestTail = result.then(
    () => undefined,
    () => undefined,
  );
  return result;
}

export function loadCachedBusinessResource<T>(
  cache: SessionPageDataCache,
  resource: CachedPageResource<T>,
  load: () => Promise<T>,
  options?: { force?: boolean },
): Promise<T> {
  return loadCachedPageResource(
    resource,
    () => queueBusinessRequest(cache, load),
    options,
  );
}
